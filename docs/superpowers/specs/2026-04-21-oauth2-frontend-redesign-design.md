# OAuth 2.0 Frontend Redesign — Design Spec (Phase 2 of 2)

**Date:** 2026-04-21
**Status:** Approved
**Feature:** Full Bruno-parity OAuth2 — Frontend AuthEditor redesign
**Prerequisite:** Phase 1 (Rust Foundation) complete

---

## Context

Phase 1 consolidated all OAuth2 token logic into Rust with three Tauri commands
(`oauth2_get_token`, `oauth2_refresh_token`, `oauth2_decode_jwt`), full 7-scope
variable resolution, additional parameters, JWT decoding, PKCE toggle, and system
browser support.

Phase 2 redesigns the frontend AuthEditor OAuth2 section to match Bruno's layout:
distinct visual sections (Configuration, Token, Advanced Settings, Settings,
Additional Parameters), collapsible token display panels, and wiring to the Phase 1
Rust commands. The current JS-based token acquisition in `handleGetToken` and
`handleRefreshToken` is replaced entirely by Tauri command calls.

All OAuth2 configuration persists to `.yml` files via the existing DDD architecture:
frontend maps flat UI state to domain `Auth::OAuth2(OAuth2Flow)` at the IPC boundary,
domain types flow through `rocket-app` services, and `rocket-infra`'s `oc_conversions.rs`
handles the domain↔OpenCollection YAML mapping.

---

## 1. Component Architecture

The current `AuthEditor.tsx` renders all OAuth2 fields inline (~600 lines total).
Adding Bruno's features inline would push it past 1000 lines. Split into focused
sub-components under a new `src/components/request/oauth2/` directory:

```
AuthEditor.tsx
  └── OAuth2AuthEditor.tsx              (orchestrator — renders sections in order)
        ├── OAuth2TokenDisplay.tsx       (collapsible Access Token + ID Token panels)
        ├── OAuth2ConfigSection.tsx      (Configuration: URLs, credentials, scope, PKCE, system browser)
        ├── OAuth2TokenSection.tsx       (Token: source, ID, add-to, header prefix)
        ├── OAuth2AdvancedSection.tsx    (Advanced: refresh token URL)
        ├── OAuth2SettingsSection.tsx    (Settings: auto-fetch, auto-refresh checkboxes)
        └── OAuth2AdditionalParams.tsx   (Additional Parameters: tabbed key-value editor)
```

Each sub-component receives:
- `oauth2: NonNullable<AuthState['oauth2']>` — the current OAuth2 state
- `patchOAuth2: (patch: Partial<...>) => void` — state update callback
- `variableContext?: Map<string, VariableScopeEntry>` — for SingleLineEditor
- `onNavigateToSource?: (...)` — for variable navigation

`OAuth2AuthEditor` additionally receives resolution context props for Tauri commands:
- `collection?: string`
- `environmentName?: string`
- `requestPath?: string`

---

## 2. AuthState Type Extensions

In `src/types/pane-types.ts`, extend the `oauth2` type:

```typescript
oauth2?: {
  // --- Existing fields (unchanged) ---
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
  verifySsl: boolean;

  // --- New fields ---

  // Options
  usePkce: boolean;                    // "Use PKCE" checkbox (default: true)
  useSystemBrowser: boolean;           // "Use system browser for OAuth" checkbox (default: false)

  // Token section
  tokenSource: 'accessToken' | 'idToken';  // Token Source dropdown (default: 'accessToken')
  tokenId: string;                     // Token ID label (e.g. "Sage ID user")

  // Advanced Settings
  refreshTokenUrl: string;             // Separate refresh token URL

  // Settings
  autoFetchToken: boolean;             // "Automatically fetch token if not found" (default: true)
  autoRefreshToken: boolean;           // "Auto refresh token (with refresh URL)" (default: false)

  // Additional Parameters (3 phases)
  authParams: AdditionalParam[];       // Authorization phase params
  tokenParams: AdditionalParam[];      // Token exchange phase params
  refreshParams: AdditionalParam[];    // Refresh phase params

  // Token response storage (ephemeral — NOT persisted to .yml)
  idToken: string;                     // Raw ID token JWT string
  tokenType: string;                   // e.g. "Bearer" from token response
  responseScope: string;               // Scope returned in token response
  idTokenClaims: JwtClaims | null;     // Decoded ID token claims
};
```

Supporting types:

```typescript
interface AdditionalParam {
  key: string;
  value: string;
  sendIn: 'queryparams' | 'body';
  enabled: boolean;
}

interface JwtClaims {
  subject: string | null;
  issuer: string | null;
  audience: string | null;
  expiry: number | null;
  issuedAt: number | null;
  scope: string | null;
  tokenType: string | null;
  algorithm: string | null;
  rawPayload: string;
}
```

Default values when switching to OAuth2:

```typescript
{
  // ... existing defaults ...
  usePkce: true,
  useSystemBrowser: false,
  tokenSource: 'accessToken',
  tokenId: '',
  refreshTokenUrl: '',
  autoFetchToken: true,
  autoRefreshToken: false,
  authParams: [],
  tokenParams: [],
  refreshParams: [],
  idToken: '',
  tokenType: '',
  responseScope: '',
  idTokenClaims: null,
}
```

---

## 3. Bruno-Style Section Layout

The OAuth2 panel renders 8 visual sections matching Bruno's layout. Each section
has an icon + bold header. Sections separated by whitespace.

### Section order:
1. **Grant Type** — dropdown
2. **Token Display** — collapsible panels (only when tokens exist)
3. **Configuration** — URLs, credentials, scope, checkboxes
4. **Token** — Token Source, Token ID, Add token to, Header Prefix
5. **Advanced Settings** — Refresh Token URL
6. **Settings** — auto-fetch, auto-refresh checkboxes
7. **Additional Parameters** — tabbed key-value editor
8. **Action Buttons** — Get Access Token, Clear Cache

### Field visibility per grant type

**Configuration section:**

| Field | client_credentials | password | auth_code | implicit |
|---|---|---|---|---|
| Callback URL | ❌ | ❌ | ✅ | ✅ |
| Use system browser | ❌ | ❌ | ✅ | ✅ |
| Authorization URL | ❌ | ❌ | ✅ | ✅ |
| Access Token URL | ✅ | ✅ | ✅ | ❌ |
| Client ID | ✅ | ✅ | ✅ | ✅ |
| Client Secret | ✅ | ✅ | ✅ | ❌ |
| Scope | ✅ | ✅ | ✅ | ✅ |
| State | ❌ | ❌ | ✅ | ✅ |
| Add Credentials to | ✅ | ✅ | ✅ | ❌ |
| Use PKCE | ❌ | ❌ | ✅ | ❌ |
| Username / Password | ❌ | ✅ | ❌ | ❌ |

**Section visibility:**

| Section | client_credentials | password | auth_code | implicit |
|---|---|---|---|---|
| Token Display | ✅ (if token) | ✅ (if token) | ✅ (if token) | ✅ (if token) |
| Configuration | ✅ (subset) | ✅ (subset) | ✅ (all) | ✅ (no token URL) |
| Token | ✅ | ✅ | ✅ | ✅ |
| Advanced Settings | ✅ | ✅ | ✅ | ❌ |
| Settings | ✅ | ✅ | ✅ | ✅ |
| Additional Params | ✅ | ✅ | ✅ | ✅ (auth tab only) |

---

## 4. Token Display Component

`OAuth2TokenDisplay.tsx` renders collapsible panels using shadcn `Collapsible`.

### Access Token panel

```
▼ Access Token                           Expires in 7h 40m
  ┌─────────────────────────────────────────────────────┐
  │ ya29.a0AW...long token string...                    │  [Copy]
  └─────────────────────────────────────────────────────┘
```

- Header row: "Access Token" label + expiry countdown (green/red)
- Expanded: read-only input with token value + Copy button
- Expiry updates via `useEffect` with 30-second interval

### ID Token panel

```
▼ ID Token
  Subject:    user123
  Issuer:     https://auth.example.com
  Audience:   my-client-id
  Expires:    2026-04-21 18:30:00
  Issued At:  2026-04-21 11:30:00
  Algorithm:  RS256

  [View Raw Payload]
```

- Only shown when `idToken` is non-empty
- Displays decoded `JwtClaims` fields
- "View Raw Payload" toggles showing `rawPayload` JSON

### Summary row

Below panels: "Token Type: Bearer" and "Scope: openid profile email" as muted text.

### JWT decoding trigger

When `oauth2_get_token` returns an `idToken`, the frontend calls `oauth2_decode_jwt`
and stores the result in `idTokenClaims`. Decoding failure is non-critical — the
ID Token panel falls back to showing the raw string.

---

## 5. Additional Parameters Component

`OAuth2AdditionalParams.tsx` — tabbed key-value editor using shadcn `Tabs`.

### Three tabs: Authorization, Token, Refresh

Each tab renders a table:

| Key | Value | Send In | ✅ | 🗑 |
|---|---|---|---|---|
| SingleLineEditor | SingleLineEditor | Select (queryparams/body) | Checkbox | Trash2 icon |

"+ Add Parameter" button at bottom adds `{ key: '', value: '', sendIn: 'queryparams', enabled: true }`.

### Tab visibility per grant type

| Tab | client_credentials | password | auth_code | implicit |
|---|---|---|---|---|
| Authorization | ❌ | ❌ | ✅ | ✅ |
| Token | ✅ | ✅ | ✅ | ❌ |
| Refresh | ✅ | ✅ | ✅ | ✅ |

Hidden tabs are not rendered (not just disabled).

---

## 6. Settings & Auto-Fetch/Auto-Refresh

### UI

`OAuth2SettingsSection.tsx` — two shadcn `Checkbox` components with help tooltips:

- "Automatically fetch token if not found" (`autoFetchToken`, default: true)
- "Auto refresh token (with refresh URL)" (`autoRefreshToken`, default: false)

### Behavior in `execute-request.ts`

Before sending a request with OAuth2 auth:

```
if autoRefreshToken && token expired && refreshToken && (refreshTokenUrl || tokenUrl):
  → call oauth2RefreshToken(...) via Tauri
  → update pane store with new token
else if autoFetchToken && no accessToken:
  → call oauth2GetToken(...) via Tauri
  → update pane store with new token
→ proceed with normal request execution
```

If auto-fetch/auto-refresh fails, log the error and proceed without a token.
The user sees the auth error in the response.

---

## 7. Persistence — DDD-Compliant Mapping

### Layer responsibilities

| Layer | Type | Responsibility |
|---|---|---|
| Frontend | `AuthState.oauth2` (flat) | UI rendering |
| Frontend | `oauth2-mapping.ts` | Flat UI state ↔ domain `Auth` at IPC boundary |
| src-tauri | `Auth::OAuth2(OAuth2Flow)` | Receives domain type via serde |
| rocket-app | `Auth::OAuth2(OAuth2Flow)` | Orchestration, variable resolution |
| rocket-infra | `OcAuthTyped::OAuth2` ↔ `Auth::OAuth2` | `oc_conversions.rs` handles domain↔YAML |
| rocket-shared | `OAuth2Flow` enum | Domain truth — type definitions |

### Frontend mapping (`src/lib/oauth2-mapping.ts`)

```typescript
// Save path: flat UI state → domain Auth IPC shape
export function oauth2StateToApiAuth(state: OAuth2State): ApiAuth

// Load path: domain Auth IPC shape → flat UI state
export function apiAuthToOAuth2State(auth: ApiAuth): OAuth2State
```

Field mapping examples:

| Frontend flat field | Domain type field |
|---|---|
| `grantType: "authorization_code"` | `OAuth2Flow::AuthorizationCode { ... }` |
| `clientId` + `clientSecret` + `clientAuthentication` | `OAuth2ClientCredentials { client_id, client_secret, placement }` |
| `username` + `password` | `OAuth2ResourceOwner { username, password }` |
| `usePkce` | `OAuth2PKCE { enabled, method: Some("S256") }` |
| `authParams` / `tokenParams` / `refreshParams` | `OAuth2AdditionalParameters { authorization_request, access_token_request, refresh_token_request }` |
| `tokenSource` + `tokenId` + `addTokenTo` + `headerPrefix` | `OAuth2TokenConfig { id, placement }` |
| `autoFetchToken` + `autoRefreshToken` + `verifySsl` | `OAuth2Settings { ... }` |
| `refreshTokenUrl` | `refresh_token_url` on each flow variant |

### AdditionalParam mapping

Frontend `sendIn` ↔ Domain `placement`:
- `"queryparams"` ↔ `"query"`
- `"body"` ↔ `"body"`

### Infra layer verification

`crates/rocket-infra/src/oc_conversions.rs` must roundtrip all fields in
`Auth::OAuth2(OAuth2Flow)` ↔ `OcAuthTyped::OAuth2`. Verify that these are
handled:
- `OAuth2AdditionalParameters` (3 phase lists)
- `OAuth2Settings` (auto_fetch_token, auto_refresh_token, verify_ssl)
- `OAuth2TokenConfig` (id, placement)
- `OAuth2PKCE` (enabled, method)
- `refresh_token_url` on each flow variant

If any field is missing from the conversion, add it in `oc_conversions.rs`.

### What persists to `.yml`

Configuration fields only:

```yaml
auth:
  type: oauth2
  flow: authorization_code
  authorizationUrl: "https://id-shadow.sage.com/authorize"
  accessTokenUrl: "https://id-shadow.sage.com/oauth/token"
  refreshTokenUrl: ""
  callbackUrl: "https://jwt.io/"
  credentials:
    clientId: "{{client_id}}"
    clientSecret: "******************"
    placement: basic_auth_header
  scope: "openid email profile offline_access user:full"
  state: "abcdef123456789"
  pkce:
    enabled: true
    method: S256
  additionalParameters:
    authorizationRequest:
      - name: nonce
        value: iamarandomnonce
        placement: query
      - name: audience
        value: snet-dev/network/api
        placement: query
  tokenConfig:
    id: "Sage ID user"
    placement:
      header: Authorization
  settings:
    autoFetchToken: true
    autoRefreshToken: false
    verifySsl: true
```

### What does NOT persist (ephemeral runtime state)

`accessToken`, `refreshToken`, `expiresIn`, `tokenAcquiredAt`, `idToken`,
`idTokenClaims`, `tokenType`, `responseScope` — stored only in the frontend
Zustand pane store. Lost on app restart, just like Bruno.

---

## 8. Tauri API Wiring

### New functions in `src/lib/tauri-api.ts`

```typescript
export interface OAuth2GetTokenRequest {
  grantType: string;
  authorizationUrl?: string;
  tokenUrl?: string;
  callbackUrl?: string;
  clientId: string;
  clientSecret?: string;
  scope?: string;
  state?: string;
  username?: string;
  password?: string;
  clientAuthentication?: string;
  usePkce?: boolean;
  useSystemBrowser?: boolean;
  verifySsl?: boolean;
  authParams?: AdditionalParam[];
  tokenParams?: AdditionalParam[];
  refreshParams?: AdditionalParam[];
  collection?: string;
  environmentName?: string;
  requestPath?: string;
}

export interface OAuth2RefreshRequest {
  refreshToken: string;
  tokenUrl: string;
  refreshTokenUrl?: string;
  clientId: string;
  clientSecret?: string;
  scope?: string;
  clientAuthentication?: string;
  verifySsl?: boolean;
  refreshParams?: AdditionalParam[];
  collection?: string;
  environmentName?: string;
  requestPath?: string;
}

export interface OAuth2TokenResult {
  accessToken: string;
  tokenType: string;
  expiresIn?: number;
  refreshToken?: string;
  scope?: string;
  idToken?: string;
}

export const oauth2GetToken = (request: OAuth2GetTokenRequest) =>
  invoke<OAuth2TokenResult>('oauth2_get_token', { request });

export const oauth2RefreshToken = (request: OAuth2RefreshRequest) =>
  invoke<OAuth2TokenResult>('oauth2_refresh_token', { request });

export const oauth2DecodeJwt = (token: string) =>
  invoke<JwtClaims>('oauth2_decode_jwt', { token });
```

### Replace `handleGetToken` and `handleRefreshToken`

Current JS-based token acquisition via `executeRequest` is replaced entirely
with `oauth2GetToken` / `oauth2RefreshToken` Tauri command calls. All token
logic moves to Rust. The frontend only stores the result and calls
`oauth2DecodeJwt` for ID token display.

### Remove old code

- Delete `handleGetToken` body (replace with `oauth2GetToken` call)
- Delete `handleRefreshToken` body (replace with `oauth2RefreshToken` call)
- Remove `executeRequest` import from `AuthEditor` (no longer needed for token acquisition)
- Keep `oauth2AuthCodeFlow` import temporarily for backward compat until Phase 2 is fully wired

---

## 9. File Map

### New files

| File | Purpose |
|---|---|
| `src/components/request/oauth2/OAuth2AuthEditor.tsx` | Parent orchestrator |
| `src/components/request/oauth2/OAuth2TokenDisplay.tsx` | Collapsible token panels |
| `src/components/request/oauth2/OAuth2ConfigSection.tsx` | Configuration fields |
| `src/components/request/oauth2/OAuth2TokenSection.tsx` | Token source, ID, placement |
| `src/components/request/oauth2/OAuth2AdvancedSection.tsx` | Refresh Token URL |
| `src/components/request/oauth2/OAuth2SettingsSection.tsx` | Auto-fetch/refresh checkboxes |
| `src/components/request/oauth2/OAuth2AdditionalParams.tsx` | Tabbed key-value editor |
| `src/lib/oauth2-mapping.ts` | `oauth2StateToApiAuth` / `apiAuthToOAuth2State` |

### Modified files

| File | Change |
|---|---|
| `src/types/pane-types.ts` | Extend oauth2 type with new fields + AdditionalParam + JwtClaims |
| `src/components/request/AuthEditor.tsx` | Replace inline OAuth2 with `<OAuth2AuthEditor>` |
| `src/lib/tauri-api.ts` | Add oauth2GetToken, oauth2RefreshToken, oauth2DecodeJwt |
| `src/lib/execute-request.ts` | Auto-fetch/refresh logic, remove old JS token code |
| `crates/rocket-infra/src/oc_conversions.rs` | Verify/fix roundtrip for all OAuth2 fields |

---

## 10. Out of Scope

- Token persistence across app restarts (tokens are ephemeral)
- Token management (save/name/list multiple tokens like Postman)
- JWKS-based signature verification for ID tokens
- Implicit flow full implementation (stub returns error — use auth code + PKCE instead)
- `.well-known/openid-configuration` auto-discovery
