// Allows optional whitespace inside braces (e.g., {{ token }}) so that hand-typed
// variables are highlighted even with padding. The URL bar uses a stricter regex
// without whitespace tolerance; this divergence is intentional.
const VAR_REGEX = /\{\{\s*([\w.-]+)\s*\}\}/g;

export interface TextToken {
  type: 'text' | 'variable';
  /** Raw text content, or variable name without braces (whitespace trimmed). */
  content: string;
  /** Characters consumed in the original string, including `{{` and `}}` braces for variable tokens. */
  rawLength: number;
}

/**
 * Tokenizes arbitrary text into plain-text and {{variable}} segments.
 * Example: "Bearer {{token}}" → [{type:'text', content:'Bearer ', rawLength:7}, {type:'variable', content:'token', rawLength:9}]
 */
export function parseTextTokens(text: string): TextToken[] {
  const tokens: TextToken[] = [];
  let lastIndex = 0;

  for (const match of text.matchAll(VAR_REGEX)) {
    const matchStart = match.index ?? 0;
    if (matchStart > lastIndex) {
      const content = text.slice(lastIndex, matchStart);
      tokens.push({ type: 'text', content, rawLength: content.length });
    }
    tokens.push({ type: 'variable', content: match[1], rawLength: match[0].length });
    lastIndex = matchStart + match[0].length;
  }

  if (lastIndex < text.length) {
    const content = text.slice(lastIndex);
    tokens.push({ type: 'text', content, rawLength: content.length });
  }

  return tokens;
}
