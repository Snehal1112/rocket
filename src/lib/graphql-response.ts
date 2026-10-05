export interface GraphQlError {
  message: string;
  path?: (string | number)[];
  locations?: { line: number; column: number }[];
  extensions?: Record<string, unknown>;
}

export interface ParsedGraphQlResponse {
  /** True when the body is a JSON object with a `data` or `errors` member. */
  isGraphQl: boolean;
  data: unknown;
  errors: GraphQlError[];
}

function toError(raw: unknown): GraphQlError {
  if (typeof raw === 'string') return { message: raw };
  if (raw && typeof raw === 'object') {
    const e = raw as Record<string, unknown>;
    return {
      message: typeof e.message === 'string' ? e.message : 'Unknown error',
      path: Array.isArray(e.path) ? (e.path as (string | number)[]) : undefined,
      locations: Array.isArray(e.locations)
        ? (e.locations as { line: number; column: number }[])
        : undefined,
      extensions:
        e.extensions && typeof e.extensions === 'object'
          ? (e.extensions as Record<string, unknown>)
          : undefined,
    };
  }
  return { message: 'Unknown error' };
}

// Reads a GraphQL-over-HTTP response body. Anything that is not a JSON object
// with `data` or `errors` is reported as not GraphQL, so the Data and Errors
// tabs fall back to the plain body view.
export function parseGraphQlResponse(body: string): ParsedGraphQlResponse {
  const empty: ParsedGraphQlResponse = { isGraphQl: false, data: undefined, errors: [] };
  let parsed: unknown;
  try {
    parsed = JSON.parse(body);
  } catch {
    return empty;
  }
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return empty;
  const obj = parsed as Record<string, unknown>;
  if (!('data' in obj) && !('errors' in obj)) return empty;
  return {
    isGraphQl: true,
    data: obj.data,
    errors: Array.isArray(obj.errors) ? obj.errors.map(toError) : [],
  };
}
