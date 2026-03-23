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
      key = decodeURIComponent(pair);
      value = '';
    } else {
      key = decodeURIComponent(pair.slice(0, eqIdx));
      value = decodeURIComponent(pair.slice(eqIdx + 1));
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
// Disabled entries are excluded. Keys and values are percent-encoded.
export function buildUrl(baseUrl: string, params: KeyValueEntry[]): string {
  const enabled = params.filter((p) => p.enabled);
  if (enabled.length === 0) {
    return baseUrl;
  }

  const queryString = enabled
    .map((p) => `${encodeURIComponent(p.key)}=${encodeURIComponent(p.value)}`)
    .join('&');

  return `${baseUrl}?${queryString}`;
}

// Extract named path parameters from a URL path (before any query string).
// Supports :param and {param} patterns.
export function extractPathParams(url: string): string[] {
  const { base } = splitUrl(url);
  const results: string[] = [];

  // Match :paramName — stops at / ? & # or end of string.
  const colonPattern = /:([A-Za-z_][A-Za-z0-9_]*)/g;
  let match: RegExpExecArray | null;
  while ((match = colonPattern.exec(base)) !== null) {
    results.push(match[1]);
  }

  // Match {paramName}.
  const bracePattern = /\{([A-Za-z_][A-Za-z0-9_]*)\}/g;
  while ((match = bracePattern.exec(base)) !== null) {
    results.push(match[1]);
  }

  return results;
}
