import type { KeyValueEntry } from '@/types/pane-types';

// Split a URL into the base portion and its raw query string.
export function splitUrl(url: string): { base: string; queryString: string } {
  const questionIdx = url.indexOf('?');
  if (questionIdx === -1) {
    return { base: url, queryString: '' };
  }
  return {
    base: url.slice(0, questionIdx),
    queryString: url.slice(questionIdx + 1),
  };
}

// Decodes a query component. A malformed escape, such as a lone '%', is kept as typed.
function safeDecode(text: string): string {
  try {
    return decodeURIComponent(text);
  } catch {
    return text;
  }
}

// Parse the query parameters from a URL into key-value entries.
// Duplicate keys each become a separate entry. Encoded characters are decoded.
export function parseQueryParams(url: string): KeyValueEntry[] {
  const { queryString } = splitUrl(url);
  if (!queryString) {
    return [];
  }

  return queryString.split('&').map((pair) => {
    const eqIdx = pair.indexOf('=');
    let key: string;
    let value: string;

    if (eqIdx === -1) {
      key = safeDecode(pair);
      value = '';
    } else {
      key = safeDecode(pair.slice(0, eqIdx));
      value = safeDecode(pair.slice(eqIdx + 1));
    }

    return {
      id: crypto.randomUUID(),
      key,
      value,
      enabled: true,
    };
  });
}

// Build a URL from a base string and a list of key-value entries.
// Disabled entries are excluded. Keys and values are percent-encoded unless `encode` is false,
// which keeps them as typed so that {{variables}} stay resolvable.
export function buildUrl(baseUrl: string, params: KeyValueEntry[], encode = true): string {
  const enabled = params.filter((p) => p.enabled);
  if (enabled.length === 0) {
    return baseUrl;
  }

  const queryString = enabled
    .map((p) =>
      encode
        ? `${encodeURIComponent(p.key)}=${encodeURIComponent(p.value)}`
        : `${p.key}=${p.value}`,
    )
    .join('&');

  return `${baseUrl}?${queryString}`;
}

// Extract named path parameters from a URL path (before any query string).
// Supports :param and {param} patterns.
export function extractPathParams(url: string): string[] {
  const { base } = splitUrl(url);
  // Strip {{variable}} patterns first so {param} regex doesn't match inside them.
  const stripped = base.replace(/\{\{[\w.-]+\}\}/g, '');
  const results: string[] = [];

  // Match :paramName — stops at / ? & # or end of string.
  const colonPattern = /:([A-Za-z_][A-Za-z0-9_]*)/g;
  for (const match of stripped.matchAll(colonPattern)) {
    results.push(match[1]);
  }

  // Match {paramName} (single-brace, not double-brace).
  const bracePattern = /\{([A-Za-z_][A-Za-z0-9_]*)\}/g;
  for (const match of stripped.matchAll(bracePattern)) {
    results.push(match[1]);
  }

  return results;
}

interface PathParamValue {
  name: string;
  value: string;
}

// Display-only mirror of `substitute_path_params` in rocket-http. The backend does the real
// substitution when a request is sent; this only builds the URL shown in the cURL copy and
// the console. Keep the two in step.
export function applyPathParams(url: string, params: PathParamValue[]): string {
  const usable = params.filter((p) => p.name && p.value);
  if (usable.length === 0) return url;
  const schemeAt = url.indexOf('://');
  const hasScheme = schemeAt !== -1 && !/[/?#]/.test(url.slice(0, schemeAt));
  const authorityStart = hasScheme ? schemeAt + 3 : 0;
  const pathOffset = url.slice(authorityStart).search(/[/?#]/);
  if (pathOffset === -1) return url;
  const start = authorityStart + pathOffset;
  const endOffset = url.slice(start).search(/[?#]/);
  const end = endOffset === -1 ? url.length : start + endOffset;
  const path = url
    .slice(start, end)
    .split('/')
    .map((segment) => rewriteSegment(segment, usable))
    .join('/');
  return url.slice(0, start) + path + url.slice(end);
}

function rewriteSegment(segment: string, params: PathParamValue[]): string {
  let out = segment;
  const colon = /^:([A-Za-z0-9_]+)/.exec(segment);
  if (colon) {
    const match = params.find((p) => p.name === colon[1]);
    if (match) out = encodeURIComponent(match.value) + segment.slice(colon[0].length);
  }
  for (const p of params) {
    const escaped = p.name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const pattern = new RegExp(`(?<!\\{)\\{${escaped}\\}(?!\\})`, 'g');
    out = out.replace(pattern, () => encodeURIComponent(p.value));
  }
  return out;
}
