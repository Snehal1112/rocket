import type { ExampleRequestSnapshot, ExampleResponseSnapshot } from '@/lib/tauri-api';
import type { KeyValueEntry, RequestState, ResponseState } from '@/types/pane-types';

export interface ExampleSnapshot {
  request: ExampleRequestSnapshot;
  response: ExampleResponseSnapshot;
  /** What was dropped, as user-facing sentences. */
  notes: string[];
}

/** A response body above this many characters is not stored. */
const MAX_RESPONSE_BODY = 512 * 1024;

const TEXT_BODY_MODES: RequestState['body']['mode'][] = ['json', 'text', 'xml', 'sparql'];

function pairs(rows: KeyValueEntry[]): { name: string; value: string }[] {
  return rows.filter((r) => r.enabled).map((r) => ({ name: r.key, value: r.value }));
}

function requestBody(
  body: RequestState['body'],
  notes: string[],
): ExampleRequestSnapshot['body'] | undefined {
  if (TEXT_BODY_MODES.includes(body.mode)) return { type: body.mode, data: body.content };
  if (body.mode === 'formurlencoded')
    return { type: 'form-urlencoded', data: pairs(body.formData) };
  if (body.mode === 'formdata' || body.mode === 'binary') {
    notes.push(
      'The request body is not saved because form-data and binary bodies are not supported.',
    );
  }
  return undefined;
}

function responseBodyType(response: ResponseState): string {
  const contentType =
    response.headers.find((h) => h.key.toLowerCase() === 'content-type')?.value.toLowerCase() ?? '';
  if (contentType.includes('json')) return 'json';
  if (contentType.includes('xml')) return 'xml';
  if (contentType.includes('html')) return 'html';
  return 'text';
}

/** Turns the open request and its live response into the on-disk example shape. */
export function buildExampleSnapshot(
  request: RequestState,
  response: ResponseState,
): ExampleSnapshot {
  const notes: string[] = [];

  // The URL keeps its query string, as the saved request does.
  const params: NonNullable<ExampleRequestSnapshot['params']> = [
    ...pairs(request.queryParams).map((p) => ({ ...p, type: 'query' as const })),
    ...pairs(request.pathParams).map((p) => ({ ...p, type: 'path' as const })),
  ];
  const body = requestBody(request.body, notes);
  const requestSnapshot: ExampleRequestSnapshot = {
    url: request.url,
    method: request.method,
    headers: pairs(request.headers),
    params,
    ...(body ? { body } : {}),
  };

  const dropped = response.isBinary === true || response.body.length > MAX_RESPONSE_BODY;
  if (dropped) {
    notes.push('The response body is not saved because it is binary or larger than 512 KB.');
  }
  const responseSnapshot: ExampleResponseSnapshot = {
    status: response.status,
    statusText: response.statusText,
    headers: response.headers.map((h) => ({ name: h.key, value: h.value })),
    body: dropped
      ? { type: 'binary', data: '' }
      : { type: responseBodyType(response), data: response.body },
  };

  return { request: requestSnapshot, response: responseSnapshot, notes };
}

/** The suggested example name, such as `200 OK`. */
export function defaultExampleName(response: ResponseState): string {
  return `${response.status} ${response.statusText}`.trim();
}
