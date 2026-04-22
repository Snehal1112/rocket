# OAuth2 Frontend Redesign — Plan D: Types, Mapping & Tauri API

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend `AuthState.oauth2` with all new fields, create the `oauth2-mapping.ts` bidirectional conversion layer, add new Tauri API invoke functions, and verify `oc_conversions.rs` roundtrips all OAuth2 fields.

**Architecture:** Frontend type extensions in `pane-types.ts`. New `oauth2-mapping.ts` maps flat UI state ↔ domain `Auth::OAuth2(OAuth2Flow)` at the IPC boundary. New Tauri API functions call Phase 1 Rust commands. Infra layer `oc_conversions.rs` verified for complete field roundtripping.

**Tech Stack:** TypeScript, React, Rust (rocket-infra)

**Spec:** `docs/superpowers/specs/2026-04-21-oauth2-frontend-redesign-design.md`

**Prerequisite:** Phase 1 (Plans A, B, C) complete.

---

## File Map

| File | Action | Purpose |
|---|---|---|
| `src/types/pane-types.ts` | Modify | Extend oauth2 type with new fields |
| `src/lib/oauth2-mapping.ts` | Create | Bidirectional flat↔domain conversion |
| `src/lib/tauri-api.ts` | Modify | Add oauth2GetToken, oauth2RefreshToken, oauth2DecodeJwt |
| `crates/rocket-infra/src/oc_conversions.rs` | Modify (if needed) | Verify/fix OAuth2 field roundtripping |

---

### Task 1: Extend AuthState.oauth2 type and add supporting types

**Files:**
- Modify: `src/types/pane-types.ts`

- [ ] **Step 1: Read the current oauth2 type**

```bash
grep -n "oauth2?" src/types/pane-types.ts
```

Read the surrounding block to understand the full type definition.

- [ ] **Step 2: Add `AdditionalParam` and `JwtClaims` interfaces**

Add these interfaces near the top of the file, before `AuthState`:

```typescript
export interface OAuth2AdditionalParam {
  key: string;
  value: string;
  sendIn: 'queryparams' | 'body';
  enabled: boolean;
}

export interface OAuth2JwtClaims {
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

- [ ] **Step 3: Extend the oauth2 type with new fields**

Find the `oauth2?:` block in `AuthState` and add the new fields after the existing ones:

```typescript
  // Options
  usePkce: boolean;
  useSystemBrowser: boolean;

  // Token section
  tokenSource: 'accessToken' | 'idToken';
  tokenId: string;

  // Advanced Settings
  refreshTokenUrl: string;

  // Settings
  autoFetchToken: boolean;
  autoRefreshToken: boolean;

  // Additional Parameters
  authParams: OAuth2AdditionalParam[];
  tokenParams: OAuth2AdditionalParam[];
  refreshParams: OAuth2AdditionalParam[];

  // Token response storage (ephemeral — NOT persisted)
  idToken: string;
  tokenType: string;
  responseScope: string;
  idTokenClaims: OAuth2JwtClaims | null;
```

- [ ] **Step 4: Update OAuth2 defaults**

Find where oauth2 defaults are set (in `AuthEditor.tsx` — the `setType` handler). Add defaults for new fields:

```typescript
usePkce: true,
useSystemBrowser: false,
tokenSource: 'accessToken' as const,
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
```

- [ ] **Step 5: Verify TypeScript**

```bash
npx tsc --noEmit 2>&1 | head -30
```

Expected: May show errors in AuthEditor where the new fields aren't used yet. That's expected — they'll be consumed in Plans E and F.

- [ ] **Step 6: Commit**

```bash
git add src/types/pane-types.ts src/components/request/AuthEditor.tsx
git commit -m "feat: extend OAuth2 AuthState with PKCE toggle, additional params, settings, JWT claims"
```

---

### Task 2: Create `oauth2-mapping.ts` and add Tauri API functions

**Files:**
- Create: `src/lib/oauth2-mapping.ts`
- Modify: `src/lib/tauri-api.ts`

- [ ] **Step 1: Create oauth2-mapping.ts with both conversion functions**

Create `src/lib/oauth2-mapping.ts`:

```typescript
import type { AuthState, OAuth2AdditionalParam, OAuth2JwtClaims } from '@/types/pane-types';

// Domain types as they appear over IPC (matching Rust serde output)
// These mirror Auth::OAuth2(OAuth2Flow) serialized as JSON.

interface ApiOAuth2Credentials {
  clientId: string;
  clientSecret: string;
  placement?: string | null;
}

interface ApiOAuth2ResourceOwner {
  username: string;
  password: string;
}

interface ApiOAuth2PKCE {
  enabled: boolean;
  method?: string | null;
}

interface ApiOAuth2AdditionalParameter {
  name: string;
  value: string;
  placement?: string | null;
}

interface ApiOAuth2AdditionalParameters {
  authorizationRequest?: ApiOAuth2AdditionalParameter[] | null;
  accessTokenRequest?: ApiOAuth2AdditionalParameter[] | null;
  refreshTokenRequest?: ApiOAuth2AdditionalParameter[] | null;
}

interface ApiOAuth2TokenPlacement {
  header?: string;
  query?: string;
}

interface ApiOAuth2TokenConfig {
  id?: string | null;
  placement?: ApiOAuth2TokenPlacement | null;
}

interface ApiOAuth2Settings {
  autoFetchToken?: boolean | null;
  autoRefreshToken?: boolean | null;
  verifySsl?: boolean | null;
}

// The full Auth::OAuth2(OAuth2Flow) IPC shape
export interface ApiOAuth2Auth {
  authType: 'OAuth2';
  flow: string;
  authorizationUrl?: string;
  accessTokenUrl?: string;
  refreshTokenUrl?: string;
  callbackUrl?: string;
  credentials?: ApiOAuth2Credentials;
  resourceOwner?: ApiOAuth2ResourceOwner;
  scope?: string | null;
  state?: string | null;
  pkce?: ApiOAuth2PKCE | null;
  additionalParameters?: ApiOAuth2AdditionalParameters | null;
  tokenConfig?: ApiOAuth2TokenConfig | null;
  settings?: ApiOAuth2Settings | null;
  // implicit-only
  clientId?: string;
}

type OAuth2State = NonNullable<AuthState['oauth2']>;

// ─── Frontend → IPC (save path) ─────────────────────────

function frontendParamsToApi(params: OAuth2AdditionalParam[]): ApiOAuth2AdditionalParameter[] {
  return params
    .filter((p) => p.key || p.value) // skip empty rows
    .map((p) => ({
      name: p.key,
      value: p.value,
      placement: p.sendIn === 'queryparams' ? 'query' : 'body',
    }));
}

export function oauth2StateToApiAuth(state: OAuth2State): ApiOAuth2Auth {
  const credentials: ApiOAuth2Credentials = {
    clientId: state.clientId,
    clientSecret: state.clientSecret,
    placement: state.clientAuthentication === 'header' ? 'basic_auth_header' : 'body',
  };

  const pkce: ApiOAuth2PKCE | null =
    state.grantType === 'authorization_code'
      ? { enabled: state.usePkce, method: state.usePkce ? 'S256' : null }
      : null;

  const additionalParameters: ApiOAuth2AdditionalParameters | null = (() => {
    const auth = frontendParamsToApi(state.authParams);
    const token = frontendParamsToApi(state.tokenParams);
    const refresh = frontendParamsToApi(state.refreshParams);
    if (!auth.length && !token.length && !refresh.length) return null;
    return {
      authorizationRequest: auth.length ? auth : null,
      accessTokenRequest: token.length ? token : null,
      refreshTokenRequest: refresh.length ? refresh : null,
    };
  })();

  const tokenConfig: ApiOAuth2TokenConfig | null = (() => {
    const hasId = state.tokenId.trim() !== '';
    const hasPlacement = state.addTokenTo || state.headerPrefix !== 'Bearer';
    if (!hasId && !hasPlacement) return null;
    const placement: ApiOAuth2TokenPlacement | undefined =
      state.addTokenTo === 'queryParams'
        ? { query: 'access_token' }
        : { header: state.headerPrefix || 'Bearer' };
    return {
      id: hasId ? state.tokenId : null,
      placement: placement || null,
    };
  })();

  const settings: ApiOAuth2Settings | null = {
    autoFetchToken: state.autoFetchToken,
    autoRefreshToken: state.autoRefreshToken,
    verifySsl: state.verifySsl,
  };

  const base = {
    authType: 'OAuth2' as const,
    additionalParameters,
    tokenConfig,
    settings,
  };

  switch (state.grantType) {
    case 'client_credentials':
      return {
        ...base,
        flow: 'client_credentials',
        accessTokenUrl: state.tokenUrl,
        refreshTokenUrl: state.refreshTokenUrl || undefined,
        credentials,
        scope: state.scope || null,
      };
    case 'password':
      return {
        ...base,
        flow: 'resource_owner_password_credentials',
        accessTokenUrl: state.tokenUrl,
        refreshTokenUrl: state.refreshTokenUrl || undefined,
        credentials,
        resourceOwner: { username: state.username, password: state.password },
        scope: state.scope || null,
      };
    case 'authorization_code':
      return {
        ...base,
        flow: 'authorization_code',
        authorizationUrl: state.authorizationUrl,
        accessTokenUrl: state.tokenUrl,
        refreshTokenUrl: state.refreshTokenUrl || undefined,
        callbackUrl: state.callbackUrl || undefined,
        credentials,
        scope: state.scope || null,
        state: state.state || null,
        pkce,
      };
    case 'implicit':
      return {
        ...base,
        flow: 'implicit',
        authorizationUrl: state.authorizationUrl,
        callbackUrl: state.callbackUrl || undefined,
        clientId: state.clientId,
        scope: state.scope || null,
        state: state.state || null,
      };
    default:
      return { ...base, flow: 'client_credentials' };
  }
}

// ─── IPC → Frontend (load path) ─────────────────────────

function apiParamsToFrontend(params?: ApiOAuth2AdditionalParameter[] | null): OAuth2AdditionalParam[] {
  if (!params) return [];
  return params.map((p) => ({
    key: p.name,
    value: p.value,
    sendIn: p.placement === 'query' ? 'queryparams' as const : 'body' as const,
    enabled: true,
  }));
}

export function apiAuthToOAuth2State(auth: ApiOAuth2Auth): OAuth2State {
  const creds = auth.credentials;
  const settings = auth.settings;
  const tc = auth.tokenConfig;
  const ap = auth.additionalParameters;

  const base: Partial<OAuth2State> = {
    clientId: creds?.clientId ?? auth.clientId ?? '',
    clientSecret: creds?.clientSecret ?? '',
    clientAuthentication: creds?.placement === 'basic_auth_header' ? 'header' : 'body',
    scope: auth.scope ?? '',
    state: auth.state ?? '',
    usePkce: auth.pkce?.enabled ?? true,
    useSystemBrowser: false,
    tokenSource: 'accessToken',
    tokenId: tc?.id ?? '',
    headerPrefix: tc?.placement?.header ?? 'Bearer',
    addTokenTo: tc?.placement?.query ? 'queryParams' : 'header',
    refreshTokenUrl: auth.refreshTokenUrl ?? '',
    autoFetchToken: settings?.autoFetchToken ?? true,
    autoRefreshToken: settings?.autoRefreshToken ?? false,
    verifySsl: settings?.verifySsl ?? true,
    authParams: apiParamsToFrontend(ap?.authorizationRequest),
    tokenParams: apiParamsToFrontend(ap?.accessTokenRequest),
    refreshParams: apiParamsToFrontend(ap?.refreshTokenRequest),
    // Ephemeral — always reset on load
    accessToken: '',
    refreshToken: '',
    expiresIn: null,
    tokenAcquiredAt: null,
    idToken: '',
    tokenType: '',
    responseScope: '',
    idTokenClaims: null,
  };

  switch (auth.flow) {
    case 'client_credentials':
      return {
        ...base,
        grantType: 'client_credentials',
        authorizationUrl: '',
        tokenUrl: auth.accessTokenUrl ?? '',
        callbackUrl: '',
        username: '',
        password: '',
      } as OAuth2State;
    case 'resource_owner_password_credentials':
      return {
        ...base,
        grantType: 'password',
        authorizationUrl: '',
        tokenUrl: auth.accessTokenUrl ?? '',
        callbackUrl: '',
        username: auth.resourceOwner?.username ?? '',
        password: auth.resourceOwner?.password ?? '',
      } as OAuth2State;
    case 'authorization_code':
      return {
        ...base,
        grantType: 'authorization_code',
        authorizationUrl: auth.authorizationUrl ?? '',
        tokenUrl: auth.accessTokenUrl ?? '',
        callbackUrl: auth.callbackUrl ?? '',
        username: '',
        password: '',
      } as OAuth2State;
    case 'implicit':
      return {
        ...base,
        grantType: 'implicit',
        authorizationUrl: auth.authorizationUrl ?? '',
        tokenUrl: '',
        callbackUrl: auth.callbackUrl ?? '',
        username: '',
        password: '',
      } as OAuth2State;
    default:
      return {
        ...base,
        grantType: 'client_credentials',
        authorizationUrl: '',
        tokenUrl: '',
        callbackUrl: '',
        username: '',
        password: '',
      } as OAuth2State;
  }
}
```

- [ ] **Step 2: Add Tauri API functions**

In `src/lib/tauri-api.ts`, add to the OAuth2 section:

```typescript
// ============================================================
// OAuth2 — New unified commands (Phase 2)
// ============================================================

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
  authParams?: OAuth2AdditionalParam[];
  tokenParams?: OAuth2AdditionalParam[];
  refreshParams?: OAuth2AdditionalParam[];
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
  refreshParams?: OAuth2AdditionalParam[];
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
  invoke<OAuth2JwtClaims>('oauth2_decode_jwt', { token });
```

Add the import for `OAuth2AdditionalParam` and `OAuth2JwtClaims` from `pane-types.ts` at the top of `tauri-api.ts`.

- [ ] **Step 3: Verify TypeScript**

```bash
npx tsc --noEmit 2>&1 | head -30
```

Expected: Clean or only errors from not-yet-created components.

- [ ] **Step 4: Commit**

```bash
git add src/lib/oauth2-mapping.ts src/lib/tauri-api.ts
git commit -m "feat: OAuth2 mapping layer and Tauri API functions for unified token commands"
```

---

### Task 3: Verify oc_conversions.rs OAuth2 roundtripping

**Files:**
- Modify (if needed): `crates/rocket-infra/src/oc_conversions.rs`

- [ ] **Step 1: Read the current OAuth2 conversion code**

```bash
grep -n "OAuth2\|oauth2" crates/rocket-infra/src/oc_conversions.rs | head -40
```

Then read the full `From<OcAuthTyped> for Auth` and `From<Auth> for OcAuthTyped` implementations for the OAuth2 variant.

- [ ] **Step 2: Verify all fields are mapped**

Check that these fields roundtrip correctly:
- `additional_parameters` (3 phase lists: authorization_request, access_token_request, refresh_token_request)
- `settings` (auto_fetch_token, auto_refresh_token, verify_ssl)
- `token_config` (id, placement — header or query)
- `pkce` (enabled, method)
- `refresh_token_url` on all flow variants
- `resource_owner` on password flow

For each field, verify it appears in BOTH conversion directions.

- [ ] **Step 3: Write a roundtrip test if one doesn't exist**

In the test module of `oc_conversions.rs` (or the test file for these conversions), add:

```rust
#[test]
fn oauth2_auth_code_full_roundtrip() {
    use rocket_shared::oauth2::*;
    let original = Auth::OAuth2(OAuth2Flow::AuthorizationCode {
        authorization_url: "https://auth.example.com/authorize".into(),
        access_token_url: "https://auth.example.com/token".into(),
        refresh_token_url: Some("https://auth.example.com/refresh".into()),
        callback_url: Some("https://jwt.io/".into()),
        credentials: OAuth2ClientCredentials {
            client_id: "my-client".into(),
            client_secret: "my-secret".into(),
            placement: Some("basic_auth_header".into()),
        },
        scope: Some("openid email".into()),
        state: Some("random-state".into()),
        pkce: Some(OAuth2PKCE { enabled: true, method: Some("S256".into()) }),
        additional_parameters: Some(OAuth2AdditionalParameters {
            authorization_request: Some(vec![OAuth2AdditionalParameter {
                name: "nonce".into(),
                value: "abc123".into(),
                placement: Some("query".into()),
            }]),
            access_token_request: Some(vec![OAuth2AdditionalParameter {
                name: "audience".into(),
                value: "api/v1".into(),
                placement: Some("body".into()),
            }]),
            refresh_token_request: None,
        }),
        token_config: Some(OAuth2TokenConfig {
            id: Some("Sage ID user".into()),
            placement: Some(OAuth2TokenPlacement::Header { header: "Authorization".into() }),
        }),
        settings: Some(OAuth2Settings {
            auto_fetch_token: Some(true),
            auto_refresh_token: Some(false),
            verify_ssl: Some(true),
        }),
    });

    // Auth → OcAuthTyped
    let oc: OcAuthTyped = original.clone().into();
    // OcAuthTyped → Auth
    let back: Auth = OcAuth::Typed(oc).into();

    assert_eq!(original, back);
}
```

- [ ] **Step 4: Run the test**

```bash
cargo test -p rocket-infra -- oauth2_auth_code_full_roundtrip
```

Expected: PASS if conversions are complete. If FAIL, proceed to Step 5.

- [ ] **Step 5: Fix any missing field mappings**

If the roundtrip test fails, identify which fields are dropped during conversion and add the missing mappings in both `From<OcAuthTyped> for Auth` and `From<Auth> for OcAuthTyped`.

Common issues to watch for:
- `additional_parameters` might not be mapped at all
- `token_config.placement` might lose the header/query distinction
- `settings` fields might be skipped
- `refresh_token_url` might only exist on some flow variants in the OC struct

- [ ] **Step 6: Run full test suite**

```bash
cargo test -p rocket-infra
cargo check --workspace
```

Expected: All tests pass.

- [ ] **Step 7: Commit (if changes were needed)**

```bash
git add crates/rocket-infra/src/oc_conversions.rs
git commit -m "fix: ensure full OAuth2 field roundtripping in oc_conversions"
```
