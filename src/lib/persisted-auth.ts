// Canonical conversion between the frontend's nested AuthState and the flat,
// tagged-union Auth shape persisted to disk (collection/request YAML).
//
// Do NOT reuse execute-request.ts's toApiAuth for persistence — that
// converter is intentionally lossy for the wire request (e.g. resolves
// OAuth2 down to a bearer token, and drops AWS SigV4 entirely), which is
// correct for sending a request but destroys credentials if used to decide
// what gets written to disk.
import {
  type ApiOAuth2Auth,
  apiAuthToOAuth2State,
  oauth2StateToApiAuth,
} from '@/lib/oauth2-mapping';
import type { Auth } from '@/lib/tauri-api';
import type { AuthState } from '@/types/pane-types';

/**
 * Converts a persisted/API Auth to the frontend's nested AuthState.
 *
 * `fallbackAuthType` controls what an absent auth maps to: a standalone
 * request or a collection's own default auth has no parent to inherit from
 * ('none'), while a request loaded as part of a collection tree can inherit
 * the collection's auth ('inherit').
 */
export function fromPersistedAuth(
  auth: Auth | null | undefined,
  fallbackAuthType: 'none' | 'inherit' = 'none',
): AuthState {
  if (!auth) return { authType: fallbackAuthType };
  const a = auth as Record<string, unknown>;
  const authType = a.authType as string;

  // Backend only ever emits 'o-auth2' (kebab-case tag), but tolerate a bare
  // 'oauth2' defensively in case of hand-edited YAML or a future importer.
  if (authType === 'o-auth2' || authType === 'oauth2') {
    return {
      authType: 'oauth2',
      oauth2: apiAuthToOAuth2State(auth as unknown as ApiOAuth2Auth),
    };
  }

  if (authType === 'aws-sig-v4') {
    return {
      authType: 'aws-sig-v4',
      awsSigV4: {
        accessKey: (a.accessKey as string) ?? '',
        secretKey: (a.secretKey as string) ?? '',
        region: (a.region as string) ?? '',
        service: (a.service as string) ?? '',
        sessionToken: (a.sessionToken as string) ?? '',
        profileName: (a.profileName as string) || undefined,
      },
    };
  }

  if (authType === 'basic') {
    return {
      authType: 'basic',
      basic: {
        username: (a.username as string) ?? '',
        password: (a.password as string) ?? '',
      },
    };
  }

  if (authType === 'bearer') {
    return { authType: 'bearer', bearer: { token: (a.token as string) ?? '' } };
  }

  if (authType === 'api-key') {
    return {
      authType: 'api-key',
      apiKey: {
        key: (a.key as string) ?? '',
        value: (a.value as string) ?? '',
        addTo: ((a.placement as string) ?? 'header') as 'header' | 'query',
      },
    };
  }

  return { authType: fallbackAuthType };
}

/** Converts a frontend AuthState to the flat Auth shape for disk persistence. */
export function toPersistedAuth(auth: AuthState): Auth {
  switch (auth.authType) {
    case 'none':
    case 'inherit':
      return { authType: 'none' };
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
        placement: auth.apiKey?.addTo ?? 'header',
      };
    case 'oauth2':
      if (!auth.oauth2) return { authType: 'none' };
      return oauth2StateToApiAuth(auth.oauth2) as Auth;
    case 'aws-sig-v4': {
      const a = auth.awsSigV4;
      return {
        authType: 'aws-sig-v4',
        accessKey: a?.accessKey ?? '',
        secretKey: a?.secretKey ?? '',
        region: a?.region ?? '',
        service: a?.service ?? '',
        // Option<String> on the Rust side — omit rather than persist ''.
        sessionToken: a?.sessionToken || undefined,
        profileName: a?.profileName || undefined,
      } as unknown as Auth;
    }
    default:
      return { authType: 'none' };
  }
}
