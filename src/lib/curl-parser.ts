import type { FormDataEntry } from '@/lib/tauri-api';
import type { BodyState } from '@/types/pane-types';

export interface ParsedCurl {
  method: string;
  url: string;
  headers: { key: string; value: string }[];
  body?: {
    mode: 'json' | 'xml' | 'text' | 'formdata';
    content: string;
    formData?: FormDataEntry[];
  };
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

// Parses one `--form` or `--form-string` argument (`name=value`) into an entry.
// `literal` is true for `--form-string`, whose value is never a file or a list of options.
function parseFormEntry(arg: string, literal: boolean): FormDataEntry | null {
  const eq = arg.indexOf('=');
  if (eq <= 0) return null;
  const key = arg.slice(0, eq);
  const raw = arg.slice(eq + 1);
  if (literal || !raw.startsWith('@')) {
    return { key, value: raw, entryType: 'text', enabled: true };
  }
  const [path, ...options] = raw.slice(1).split(';');
  const entry: FormDataEntry = { key, value: path, entryType: 'file', enabled: true };
  const type = options.find((o) => o.startsWith('type='));
  if (type) entry.contentType = type.slice('type='.length);
  return entry;
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
  const formData: FormDataEntry[] = [];
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
    } else if (token === '-F' || token === '--form' || token === '--form-string') {
      const entry = parseFormEntry(tokens[++i] ?? '', token === '--form-string');
      if (entry) formData.push(entry);
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
    method = bodyContent || formData.length > 0 ? 'POST' : 'GET';
  }

  // Build body with mode detection. A form body wins over -d, as curl refuses to mix them.
  let body: ParsedCurl['body'];
  if (formData.length > 0) {
    body = { mode: 'formdata', content: '', formData };
  } else if (bodyContent) {
    const contentType = headers.find((h) => h.key.toLowerCase() === 'content-type')?.value;
    body = { mode: detectBodyMode(bodyContent, contentType), content: bodyContent };
  }

  return { method, url, headers, body, auth };
}

// Turns a parsed cURL body into the body state of the editor. Form rows get their own ids.
export function bodyFromParsedCurl(body: ParsedCurl['body']): BodyState | undefined {
  if (!body) return undefined;
  if (body.mode === 'formdata') {
    return {
      mode: 'formdata',
      content: '',
      formData: (body.formData ?? []).map((entry) => ({ id: crypto.randomUUID(), ...entry })),
    };
  }
  return { mode: body.mode, content: body.content, formData: [] };
}
