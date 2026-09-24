import type { ResolvedRequestFields } from '@/lib/execute-request';
import type { Body, Header } from '@/lib/tauri-api';

// Characters that are always safe unquoted in a POSIX shell word. Anything
// else (spaces, quotes, `?`, `&`, `{`, etc.) forces single-quoting. Mirrors
// curl-parser.ts's tokenizer conventions in reverse, so paste -> edit ->
// copy round-trips stay consistent.
const SAFE_BARE = /^[a-zA-Z0-9._/:@%^=-]+$/;

function shellQuote(value: string): string {
  if (value !== '' && SAFE_BARE.test(value)) return value;
  return `'${value.replace(/'/g, "'\\''")}'`;
}

function hasContentTypeHeader(headers: Header[]): boolean {
  return headers.some((h) => h.key.toLowerCase() === 'content-type');
}

function contentTypeFor(mode: Body['mode']): string | undefined {
  switch (mode) {
    case 'json':
      return 'application/json';
    case 'xml':
      return 'application/xml';
    case 'text':
      return 'text/plain';
    default:
      return undefined;
  }
}

interface QueryParam {
  key: string;
  value: string;
  enabled: boolean;
}

function buildUrlWithQuery(url: string, queryParams: QueryParam[]): string {
  const enabled = queryParams.filter((p) => p.enabled && p.key);
  if (enabled.length === 0) return url;

  const [base, existingQuery = ''] = url.split('?');
  const params = new URLSearchParams(existingQuery);
  for (const p of enabled) {
    params.append(p.key, p.value);
  }
  const queryString = params.toString();
  return queryString ? `${base}?${queryString}` : base;
}

function buildHeaderLines(headers: Header[]): string[] {
  return headers
    .filter((h) => h.enabled && h.key)
    .map((h) => `-H ${shellQuote(`${h.key}: ${h.value}`)}`);
}

interface AuthParts {
  authFlag?: string;
  authHeaderLines: string[];
  extraQueryParams: QueryParam[];
}

function buildAuthParts(auth: ResolvedRequestFields['auth']): AuthParts {
  switch (auth.authType) {
    case 'none':
      return { authHeaderLines: [], extraQueryParams: [] };
    case 'basic':
      return {
        authFlag: `-u ${shellQuote(`${auth.username}:${auth.password}`)}`,
        authHeaderLines: [],
        extraQueryParams: [],
      };
    case 'bearer':
      return {
        authHeaderLines: [`-H ${shellQuote(`Authorization: Bearer ${auth.token}`)}`],
        extraQueryParams: [],
      };
    case 'api-key': {
      if (auth.placement === 'query') {
        return {
          authHeaderLines: [],
          extraQueryParams: [{ key: auth.key, value: auth.value, enabled: true }],
        };
      }
      return {
        authHeaderLines: [`-H ${shellQuote(`${auth.key}: ${auth.value}`)}`],
        extraQueryParams: [],
      };
    }
    case 'o-auth2': {
      const token = typeof auth.token === 'string' ? auth.token : '';
      return {
        authHeaderLines: [`-H ${shellQuote(`Authorization: Bearer ${token}`)}`],
        extraQueryParams: [],
      };
    }
    case 'aws-sig-v4':
      // Cannot be statically reproduced — requires live HMAC signing over the
      // exact request/timestamp. Omitted; callers should warn the user based
      // on the raw (pre-resolution) auth type before it collapses to 'none'.
      return { authHeaderLines: [], extraQueryParams: [] };
    default:
      return { authHeaderLines: [], extraQueryParams: [] };
  }
}

interface BodyParts {
  lines: string[];
  extraHeaderLine?: string;
}

function buildBodyParts(body: Body | undefined, headers: Header[]): BodyParts {
  if (!body || body.mode === 'none') return { lines: [] };

  switch (body.mode) {
    case 'json':
    case 'xml':
    case 'text': {
      if (!body.content) return { lines: [] };
      const lines = [`--data ${shellQuote(body.content)}`];
      let extraHeaderLine: string | undefined;
      if (!hasContentTypeHeader(headers)) {
        const contentType = contentTypeFor(body.mode);
        if (contentType) {
          extraHeaderLine = `-H ${shellQuote(`Content-Type: ${contentType}`)}`;
        }
      }
      return { lines, extraHeaderLine };
    }
    case 'formurlencoded': {
      const lines = (body.formData ?? [])
        .filter((e) => e.enabled)
        .map((e) => `--data-urlencode ${shellQuote(`${e.key}=${e.value}`)}`);
      return { lines };
    }
    case 'formdata': {
      const lines = (body.formData ?? [])
        .filter((e) => e.enabled)
        .map((e) =>
          e.entryType === 'file'
            ? `--form ${shellQuote(`${e.key}=@${e.value}`)}`
            : `--form ${shellQuote(`${e.key}=${e.value}`)}`,
        );
      return { lines };
    }
    case 'binary': {
      if (!body.content) return { lines: [] };
      return { lines: [`--data-binary ${shellQuote(`@${body.content}`)}`] };
    }
    default:
      return { lines: [] };
  }
}

// Renders a resolved request as a copy/paste-ready POSIX shell (bash/zsh/sh)
// curl command. Mirrors curl-parser.ts's parsing rules in reverse.
export function generateCurlCommand(resolved: ResolvedRequestFields, method: string): string {
  const { authFlag, authHeaderLines, extraQueryParams } = buildAuthParts(resolved.auth);
  const url = buildUrlWithQuery(resolved.url, [...resolved.queryParams, ...extraQueryParams]);
  const headerLines = buildHeaderLines(resolved.headers);
  const { lines: bodyLines, extraHeaderLine } = buildBodyParts(resolved.body, resolved.headers);

  const parts: string[] = [`curl -X ${method.toUpperCase()} ${shellQuote(url)}`];
  if (authFlag) parts.push(authFlag);
  parts.push(...authHeaderLines);
  if (extraHeaderLine) parts.push(extraHeaderLine);
  parts.push(...headerLines);
  parts.push(...bodyLines);

  return parts.join(' \\\n  ');
}
