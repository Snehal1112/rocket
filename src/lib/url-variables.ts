// Matches {{variable.name}} style placeholders.
const VAR_REGEX = /\{\{([\w.-]+)\}\}/g;
// Matches :paramName between / delimiters or at end of path.
const PATH_PARAM_REGEX = /:(\w+)/g;

export interface UrlToken {
  type: 'text' | 'variable' | 'pathParam' | 'queryKey' | 'queryValue';
  value: string;
  start: number;
  end: number;
  resolved?: string;
  source?: string;
}

// Parses text segments for :pathParam tokens in the path portion (before ?).
function expandPathParams(
  text: string,
  offset: number,
  pathParams?: Record<string, string>,
): UrlToken[] {
  const tokens: UrlToken[] = [];
  let lastIdx = 0;

  for (const match of text.matchAll(PATH_PARAM_REGEX)) {
    const matchStart = match.index!;
    if (matchStart > lastIdx) {
      tokens.push({ type: 'text', value: text.slice(lastIdx, matchStart), start: offset + lastIdx, end: offset + matchStart });
    }
    const paramName = match[1];
    const resolved = pathParams && paramName in pathParams ? pathParams[paramName] : undefined;
    tokens.push({
      type: 'pathParam',
      value: paramName,
      start: offset + matchStart,
      end: offset + matchStart + match[0].length,
      resolved,
      source: resolved !== undefined ? 'Path Params' : undefined,
    });
    lastIdx = matchStart + match[0].length;
  }

  if (lastIdx < text.length) {
    tokens.push({ type: 'text', value: text.slice(lastIdx), start: offset + lastIdx, end: offset + text.length });
  }

  return tokens;
}

// Parses query string segments into key=value tokens.
function expandQueryTokens(
  queryString: string,
  offset: number,
  queryParams?: Record<string, string>,
): UrlToken[] {
  const tokens: UrlToken[] = [];
  const pairs = queryString.split(/(&)/);
  let pos = 0;

  for (const segment of pairs) {
    if (segment === '&') {
      tokens.push({ type: 'text', value: '&', start: offset + pos, end: offset + pos + 1 });
      pos += 1;
      continue;
    }
    const eqIdx = segment.indexOf('=');
    if (eqIdx > 0) {
      const key = segment.slice(0, eqIdx);
      const val = segment.slice(eqIdx + 1);
      const resolved = queryParams && key in queryParams ? queryParams[key] : undefined;
      tokens.push({
        type: 'queryKey',
        value: key,
        start: offset + pos,
        end: offset + pos + key.length,
        resolved,
        source: resolved !== undefined ? 'Query Params' : undefined,
      });
      tokens.push({ type: 'text', value: '=', start: offset + pos + key.length, end: offset + pos + key.length + 1 });
      if (val) {
        tokens.push({
          type: 'queryValue',
          value: val,
          start: offset + pos + key.length + 1,
          end: offset + pos + segment.length,
        });
      }
    } else if (segment) {
      tokens.push({ type: 'text', value: segment, start: offset + pos, end: offset + pos + segment.length });
    }
    pos += segment.length;
  }

  return tokens;
}

// Parses a URL string into tokens: {{variables}}, :pathParams, and query key=value pairs.
export function parseUrlTokens(
  url: string,
  envVariables: Record<string, string>,
  envName?: string,
  collectionVariables?: Record<string, string>,
  pathParams?: Record<string, string>,
  queryParams?: Record<string, string>,
): UrlToken[] {
  // First pass: split on {{var}} patterns.
  const varTokens: UrlToken[] = [];
  let lastIndex = 0;

  for (const match of url.matchAll(VAR_REGEX)) {
    const matchStart = match.index!;
    if (matchStart > lastIndex) {
      varTokens.push({ type: 'text', value: url.slice(lastIndex, matchStart), start: lastIndex, end: matchStart });
    }
    const varName = match[1];
    let resolved: string | undefined;
    let source: string | undefined;
    if (varName in envVariables) {
      resolved = envVariables[varName];
      source = envName;
    } else if (collectionVariables && varName in collectionVariables) {
      resolved = collectionVariables[varName];
      source = 'Collection';
    }
    varTokens.push({ type: 'variable', value: varName, start: matchStart, end: matchStart + match[0].length, resolved, source });
    lastIndex = matchStart + match[0].length;
  }
  if (lastIndex < url.length) {
    varTokens.push({ type: 'text', value: url.slice(lastIndex), start: lastIndex, end: url.length });
  }

  // Second pass: expand text segments for :pathParam and query tokens.
  const finalTokens: UrlToken[] = [];
  for (const token of varTokens) {
    if (token.type !== 'text') {
      finalTokens.push(token);
      continue;
    }

    const qIdx = token.value.indexOf('?');
    if (qIdx >= 0) {
      const pathPart = token.value.slice(0, qIdx);
      if (pathPart) {
        finalTokens.push(...expandPathParams(pathPart, token.start, pathParams));
      }
      finalTokens.push({ type: 'text', value: '?', start: token.start + qIdx, end: token.start + qIdx + 1 });
      const queryPart = token.value.slice(qIdx + 1);
      if (queryPart) {
        finalTokens.push(...expandQueryTokens(queryPart, token.start + qIdx + 1, queryParams));
      }
    } else {
      finalTokens.push(...expandPathParams(token.value, token.start, pathParams));
    }
  }

  return finalTokens;
}

// Builds a resolve function that substitutes {{var}} with merged variables.
export function buildResolver(
  envVariables: Record<string, string>,
  collectionVariables?: Record<string, string>,
): (text: string) => string {
  return (text: string) =>
    text.replace(VAR_REGEX, (match, key) => {
      if (key in envVariables) return envVariables[key];
      if (collectionVariables && key in collectionVariables) return collectionVariables[key];
      return match;
    });
}
