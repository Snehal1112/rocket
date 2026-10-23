import type { RequestExample } from '@/lib/tauri-api';
import { extractPathParams, parseQueryParams } from '@/lib/url-params';
import type { BodyState, KeyValueEntry, RequestState } from '@/types/pane-types';

type NameValue = { name: string; value: string };

function rows(items: NameValue[]): KeyValueEntry[] {
  return items.map((i) => ({
    id: crypto.randomUUID(),
    key: i.name,
    value: i.value,
    enabled: true,
  }));
}

// Maps a stored body to the editor body. A body of a kind the editor cannot show is no body.
function bodyFrom(
  stored: NonNullable<RequestExample['request']>['body'],
  fallback: BodyState,
): BodyState {
  if (!stored) return fallback;
  if (stored.type === 'form-urlencoded') {
    const data = Array.isArray(stored.data) ? stored.data : [];
    return { mode: 'formurlencoded', content: '', formData: rows(data) };
  }
  if (
    (stored.type === 'json' ||
      stored.type === 'text' ||
      stored.type === 'xml' ||
      stored.type === 'sparql') &&
    typeof stored.data === 'string'
  ) {
    return { mode: stored.type, content: stored.data, formData: [] };
  }
  return fallback;
}

/**
 * Builds the request a Try tab starts from. Method, URL, params, headers and body come from the
 * example, with `{{vars}}` kept as written. Auth, scripts, settings, assertions and actions stay
 * the parent's. An example with no `request` keeps the base. Inside a request, an absent part
 * means empty, so Try never mixes in the parent's headers or body.
 */
export function exampleToRequestState(example: RequestExample, base: RequestState): RequestState {
  const request = example.request;
  if (!request) return base;

  const url = request.url ?? base.url;
  const stored = request.params ?? [];
  const pathValues = new Map(stored.filter((p) => p.type === 'path').map((p) => [p.name, p.value]));

  return {
    ...base,
    method: (request.method?.toUpperCase() as RequestState['method'] | undefined) ?? base.method,
    url,
    // The stored URL keeps its query string, so the rows come from it.
    queryParams: parseQueryParams(url),
    pathParams: extractPathParams(url).map((name) => ({
      id: crypto.randomUUID(),
      key: name,
      value: pathValues.get(name) ?? '',
      enabled: true,
    })),
    // The saved file omits empty headers, params and body, so absent means empty.
    headers: rows(request.headers ?? []),
    body: bodyFrom(request.body, { mode: 'none', content: '', formData: [] }),
  };
}
