export interface ParsedCurl {
  method: string;
  url: string;
  headers: { key: string; value: string }[];
  body?: { mode: 'json' | 'xml' | 'text'; content: string };
  auth?: { type: 'basic'; username: string; password: string };
}

// Tokenizes a shell-like string respecting single and double quotes.
function tokenize(input: string): string[] {
  const tokens: string[] = [];
  let current = '';
  let quote: string | null = null;
  let isEscaped = false;

  for (const ch of input) {
    if (isEscaped) {
      current += ch;
      isEscaped = false;
      continue;
    }
    if (ch === '\\' && quote !== "'") {
      isEscaped = true;
      continue;
    }
    if (ch === quote) {
      quote = null;
      continue;
    }
    if (!quote && (ch === '"' || ch === "'")) {
      quote = ch;
      continue;
    }
    if (!quote && /\s/.test(ch)) {
      if (current) tokens.push(current);
      current = '';
      continue;
    }
    current += ch;
  }
  if (current) tokens.push(current);
  return tokens;
}

// Detects body content type from Content-Type header or content shape.
function detectBodyMode(content: string, contentType?: string): 'json' | 'xml' | 'text' {
  if (contentType) {
    if (contentType.includes('json')) return 'json';
    if (contentType.includes('xml')) return 'xml';
  }
  const trimmed = content.trim();
  if (
    (trimmed.startsWith('{') && trimmed.endsWith('}')) ||
    (trimmed.startsWith('[') && trimmed.endsWith(']'))
  ) {
    return 'json';
  }
  if (trimmed.startsWith('<') && trimmed.endsWith('>')) {
    return 'xml';
  }
  return 'text';
}

// Parses a cURL command string into a structured request.
// Returns null if the input does not look like a cURL command.
export function parseCurl(input: string): ParsedCurl | null {
  // Normalize: join backslash-continuation lines, trim.
  const normalized = input
    .replace(/\\\s*\n/g, ' ')
    .replace(/\\\s*\r\n/g, ' ')
    .trim();

  // Must start with "curl" (case-insensitive).
  if (!/^curl\s/i.test(normalized)) return null;

  const tokens = tokenize(normalized);
  // Remove "curl" itself.
  tokens.shift();

  let method = '';
  let url = '';
  const headers: { key: string; value: string }[] = [];
  let bodyContent: string | undefined;
  let auth: ParsedCurl['auth'];

  let i = 0;
  while (i < tokens.length) {
    const token = tokens[i];

    if (token === '-X' || token === '--request') {
      method = tokens[++i]?.toUpperCase() ?? 'GET';
    } else if (token === '-H' || token === '--header') {
      const headerStr = tokens[++i] ?? '';
      const colonIdx = headerStr.indexOf(':');
      if (colonIdx > 0) {
        headers.push({
          key: headerStr.slice(0, colonIdx).trim(),
          value: headerStr.slice(colonIdx + 1).trim(),
        });
      }
    } else if (
      token === '-d' ||
      token === '--data' ||
      token === '--data-raw' ||
      token === '--data-binary'
    ) {
      bodyContent = tokens[++i] ?? '';
    } else if (token === '-u' || token === '--user') {
      const creds = tokens[++i] ?? '';
      const colonIdx = creds.indexOf(':');
      if (colonIdx > 0) {
        auth = {
          type: 'basic',
          username: creds.slice(0, colonIdx),
          password: creds.slice(colonIdx + 1),
        };
      }
    } else if (token === '-A' || token === '--user-agent') {
      headers.push({ key: 'User-Agent', value: tokens[++i] ?? '' });
    } else if (token.startsWith('-')) {
      // Boolean flags that take no value — do not consume the next token.
      const boolFlags = new Set([
        '-L',
        '--location',
        '-k',
        '--insecure',
        '-v',
        '--verbose',
        '-s',
        '--silent',
        '-i',
        '--include',
        '-I',
        '--head',
        '-f',
        '--fail',
        '--compressed',
        '--http1.0',
        '--http1.1',
        '--http2',
        '--no-keepalive',
        '--ipv4',
        '--ipv6',
        '--anyauth',
        '--digest',
        '--negotiate',
        '--ntlm',
        '--proxy-anyauth',
        '--proxy-digest',
        '--proxy-negotiate',
        '--proxy-ntlm',
      ]);
      // Skip the next token only for flags that take a value argument.
      if (!boolFlags.has(token) && i + 1 < tokens.length && !tokens[i + 1].startsWith('-')) {
        i++; // Skip the flag's value.
      }
    } else {
      // Positional argument — this is the URL.
      url = token;
    }
    i++;
  }

  if (!url) return null;

  // Prepend https:// if no protocol.
  if (!/^https?:\/\//i.test(url)) {
    url = `https://${url}`;
  }

  // Infer method from body presence.
  if (!method) {
    method = bodyContent ? 'POST' : 'GET';
  }

  // Build body with mode detection.
  let body: ParsedCurl['body'];
  if (bodyContent) {
    const contentType = headers.find((h) => h.key.toLowerCase() === 'content-type')?.value;
    body = { mode: detectBodyMode(bodyContent, contentType), content: bodyContent };
  }

  return { method, url, headers, body, auth };
}
