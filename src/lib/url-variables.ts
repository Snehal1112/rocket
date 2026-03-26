// Matches {{variable.name}} style placeholders.
const VAR_REGEX = /\{\{([\w.-]+)\}\}/g;

export interface UrlToken {
  type: 'text' | 'variable';
  value: string;        // raw text segment or variable name (without braces)
  start: number;        // character offset in the URL string
  end: number;          // character offset end (exclusive)
  resolved?: string;    // resolved value (only for variable tokens)
  source?: string;      // source label: env name or "Collection"
}

// Parses a URL string into alternating text and variable tokens.
// Environment variables take precedence over collection variables.
export function parseUrlTokens(
  url: string,
  envVariables: Record<string, string>,
  envName?: string,
  collectionVariables?: Record<string, string>,
): UrlToken[] {
  const tokens: UrlToken[] = [];
  let lastIndex = 0;

  for (const match of url.matchAll(VAR_REGEX)) {
    const matchStart = match.index!;
    if (matchStart > lastIndex) {
      tokens.push({ type: 'text', value: url.slice(lastIndex, matchStart), start: lastIndex, end: matchStart });
    }
    const varName = match[1];

    // Env wins over collection.
    let resolved: string | undefined;
    let source: string | undefined;
    if (varName in envVariables) {
      resolved = envVariables[varName];
      source = envName;
    } else if (collectionVariables && varName in collectionVariables) {
      resolved = collectionVariables[varName];
      source = 'Collection';
    }

    tokens.push({
      type: 'variable',
      value: varName,
      start: matchStart,
      end: matchStart + match[0].length,
      resolved,
      source,
    });
    lastIndex = matchStart + match[0].length;
  }

  if (lastIndex < url.length) {
    tokens.push({ type: 'text', value: url.slice(lastIndex), start: lastIndex, end: url.length });
  }

  return tokens;
}

// Builds a resolve function that substitutes {{var}} with merged variables.
// Environment variables take precedence over collection variables.
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
