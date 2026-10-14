import type { Auth } from '@/lib/tauri-api';

export const REDACTION_MARKER = '••••••';
// Same minimum as history and the script engine: a shorter value would blank unrelated text.
export const MIN_REDACTION_LEN = 6;

// Header values hidden by name. The name stays visible.
const SENSITIVE_HEADER_NAMES = new Set([
  'authorization',
  'proxy-authorization',
  'cookie',
  'set-cookie',
  'x-api-key',
]);

export interface HeaderPair {
  key: string;
  value: string;
}

export interface HttpExchangeText {
  url: string;
  requestHeaders: HeaderPair[];
  requestBody: string;
  responseHeaders: HeaderPair[];
  responseBody: string;
}

export interface SecretVariable {
  value: string;
  initialValue?: string;
  enabled: boolean;
  secret: boolean;
}

export interface MaskOptions {
  secretValues: string[];
  /** Name of the API key sent as a query parameter. */
  apiKeyQueryParam?: string;
  /** True when the secret set could not be built. */
  uncertain?: boolean;
}

export interface MaskedExchange {
  masked: HttpExchangeText;
  changed: boolean;
  uncertain: boolean;
}

// Keeps the enabled secret values that are long enough, longest first so that a secret
// containing a shorter one is replaced whole.
export function collectSecretValues(lists: SecretVariable[][], extra: string[] = []): string[] {
  const found = new Set<string>();
  for (const list of lists) {
    for (const variable of list) {
      if (!variable.enabled || !variable.secret) continue;
      found.add(variable.value || variable.initialValue || '');
    }
  }
  for (const value of extra) found.add(value);
  return [...found]
    .filter((v) => v.length >= MIN_REDACTION_LEN)
    .sort((a, b) => b.length - a.length);
}

// The credentials an auth block adds to a send.
export function authSecretValues(auth: Auth): string[] {
  switch (auth.authType) {
    case 'bearer':
      return [auth.token];
    case 'basic':
    case 'digest':
    case 'wsse':
    case 'ntlm':
      return [auth.password];
    case 'api-key':
      return [auth.value];
    default:
      return [];
  }
}

export function buildMaskOptions(
  auth: Auth,
  secretValues: string[],
  uncertain: boolean,
): MaskOptions {
  return {
    secretValues: [...secretValues, ...authSecretValues(auth)],
    apiKeyQueryParam:
      auth.authType === 'api-key' && auth.placement === 'query' ? auth.key : undefined,
    uncertain,
  };
}

function maskText(text: string, secrets: string[]): string {
  let out = text;
  for (const secret of secrets) out = out.split(secret).join(REDACTION_MARKER);
  return out;
}

function maskHeaders(headers: HeaderPair[], secrets: string[]): HeaderPair[] {
  return headers.map((h) => {
    if (SENSITIVE_HEADER_NAMES.has(h.key.toLowerCase())) {
      return { key: h.key, value: h.value ? REDACTION_MARKER : h.value };
    }
    return { key: h.key, value: maskText(h.value, secrets) };
  });
}

const URL_USERINFO = /^([a-z][a-z0-9+.-]*:\/\/)[^/?#@]+@/i;

function maskApiKeyParam(url: string, name: string | undefined): string {
  if (!name) return url;
  const queryStart = url.indexOf('?');
  if (queryStart < 0) return url;
  const hash = url.indexOf('#', queryStart);
  const end = hash < 0 ? url.length : hash;
  const query = url
    .slice(queryStart + 1, end)
    .split('&')
    .map((pair) => {
      const eq = pair.indexOf('=');
      if (eq < 0) return pair;
      const rawKey = pair.slice(0, eq);
      let key = rawKey;
      try {
        key = decodeURIComponent(rawKey.replace(/\+/g, ' '));
      } catch {
        // Keep the raw key when it is not valid percent-encoding.
      }
      return key === name ? `${rawKey}=${REDACTION_MARKER}` : pair;
    })
    .join('&');
  return `${url.slice(0, queryStart + 1)}${query}${url.slice(end)}`;
}

function maskUrl(url: string, secrets: string[], apiKeyParam: string | undefined): string {
  const byValue = maskText(url, secrets);
  const noCredentials = byValue.replace(URL_USERINFO, `$1${REDACTION_MARKER}@`);
  return maskApiKeyParam(noCredentials, apiKeyParam);
}

export function maskHttpExchange(input: HttpExchangeText, options: MaskOptions): MaskedExchange {
  const secrets = collectSecretValues([], options.secretValues);
  const masked: HttpExchangeText = {
    url: maskUrl(input.url, secrets, options.apiKeyQueryParam),
    requestHeaders: maskHeaders(input.requestHeaders, secrets),
    requestBody: maskText(input.requestBody, secrets),
    responseHeaders: maskHeaders(input.responseHeaders, secrets),
    responseBody: maskText(input.responseBody, secrets),
  };
  return {
    masked,
    changed: JSON.stringify(masked) !== JSON.stringify(input),
    uncertain: options.uncertain ?? false,
  };
}

type ViewableEntry = HttpExchangeText & { masked?: HttpExchangeText; uncertain?: boolean };

function rawFields(entry: HttpExchangeText): HttpExchangeText {
  return {
    url: entry.url,
    requestHeaders: entry.requestHeaders,
    requestBody: entry.requestBody,
    responseHeaders: entry.responseHeaders,
    responseBody: entry.responseBody,
  };
}

// The text a row shows. An uncertain row is never revealed.
export function selectHttpView(entry: ViewableEntry, revealed: boolean): HttpExchangeText {
  if (!entry.masked) return rawFields(entry);
  if (revealed && !entry.uncertain) return rawFields(entry);
  return entry.masked;
}

export function hasHiddenValues(entry: ViewableEntry): boolean {
  return !!entry.masked && JSON.stringify(entry.masked) !== JSON.stringify(rawFields(entry));
}
