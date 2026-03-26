// Matches {{variable.name}} style placeholders.
const VAR_REGEX = /\{\{([\w.-]+)\}\}/g;

export interface UrlToken {
  type: 'text' | 'variable';
  value: string;        // raw text segment or variable name (without braces)
  start: number;        // character offset in the URL string
  end: number;          // character offset end (exclusive)
  resolved?: string;    // resolved value (only for variable tokens)
  source?: string;      // environment name (only for resolved variables)
}

// Parses a URL string into alternating text and variable tokens.
export function parseUrlTokens(
  url: string,
  variables: Record<string, string>,
  envName?: string,
): UrlToken[] {
  const tokens: UrlToken[] = [];
  let lastIndex = 0;

  for (const match of url.matchAll(VAR_REGEX)) {
    const matchStart = match.index!;
    // Add preceding text segment.
    if (matchStart > lastIndex) {
      tokens.push({ type: 'text', value: url.slice(lastIndex, matchStart), start: lastIndex, end: matchStart });
    }
    const varName = match[1];
    const resolved = varName in variables ? variables[varName] : undefined;
    tokens.push({
      type: 'variable',
      value: varName,
      start: matchStart,
      end: matchStart + match[0].length,
      resolved,
      source: resolved !== undefined ? envName : undefined,
    });
    lastIndex = matchStart + match[0].length;
  }

  // Add trailing text segment.
  if (lastIndex < url.length) {
    tokens.push({ type: 'text', value: url.slice(lastIndex), start: lastIndex, end: url.length });
  }

  return tokens;
}
