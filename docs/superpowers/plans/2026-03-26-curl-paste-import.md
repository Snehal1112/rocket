# cURL Paste Import Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Detect cURL commands pasted into the URL input, parse them into structured requests, and populate the request editor with method, URL, headers, body, and auth.

**Architecture:** Task 1 creates a pure cURL parser. Task 2 adds an `onPaste` handler to VariableAwareUrlInput that detects cURL and calls a new `onCurlImport` callback. Task 3 wires the callback in RequestPanel to populate the request state.

**Tech Stack:** React, TypeScript, Tailwind CSS, Zustand (`yarn tsc --noEmit` for verification)

**Spec:** `docs/superpowers/specs/2026-03-26-curl-paste-import-design.md`

---

## File Map

| File | Role |
|---|---|
| `src/lib/curl-parser.ts` | Create — pure cURL command parser |
| `src/components/request/VariableAwareUrlInput.tsx` | Add `onPaste` handler + `onCurlImport` callback prop |
| `src/components/request/RequestPanel.tsx` | Handle `onCurlImport` → populate request state |

---

### Task 1: Create cURL parser

**Files:**
- Create: `src/lib/curl-parser.ts`

- [ ] **Step 1: Create the parser**

Create `src/lib/curl-parser.ts` with:

```ts
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
  let escape = false;

  for (const ch of input) {
    if (escape) {
      current += ch;
      escape = false;
      continue;
    }
    if (ch === '\\' && quote !== "'") {
      escape = true;
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
  if ((trimmed.startsWith('{') && trimmed.endsWith('}')) || (trimmed.startsWith('[') && trimmed.endsWith(']'))) {
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
    } else if (token === '-d' || token === '--data' || token === '--data-raw' || token === '--data-binary') {
      bodyContent = tokens[++i] ?? '';
    } else if (token === '-u' || token === '--user') {
      const creds = tokens[++i] ?? '';
      const colonIdx = creds.indexOf(':');
      if (colonIdx > 0) {
        auth = { type: 'basic', username: creds.slice(0, colonIdx), password: creds.slice(colonIdx + 1) };
      }
    } else if (token === '-A' || token === '--user-agent') {
      headers.push({ key: 'User-Agent', value: tokens[++i] ?? '' });
    } else if (token.startsWith('-')) {
      // Skip unknown flags. If the next token doesn't start with -, it's the flag's value.
      if (i + 1 < tokens.length && !tokens[i + 1].startsWith('-')) {
        i++; // Skip the value too.
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
```

- [ ] **Step 2: Verify types**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -10
```

- [ ] **Step 3: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/lib/curl-parser.ts
git commit -m "feat: add cURL command parser with method, header, body, and auth extraction"
```

---

### Task 2: Add paste detection to VariableAwareUrlInput

**Files:**
- Modify: `src/components/request/VariableAwareUrlInput.tsx`

**Depends on:** Task 1

- [ ] **Step 1: Add import and callback prop**

Find the imports at the top of `src/components/request/VariableAwareUrlInput.tsx`. Add:

```tsx
import { parseCurl, type ParsedCurl } from '@/lib/curl-parser';
```

Find the `VariableAwareUrlInputProps` interface:
```tsx
interface VariableAwareUrlInputProps {
  value: string;
  onChange: (value: string) => void;
  onKeyDown?: React.KeyboardEventHandler<HTMLInputElement>;
  placeholder?: string;
  className?: string;
}
```

Replace with:
```tsx
interface VariableAwareUrlInputProps {
  value: string;
  onChange: (value: string) => void;
  onKeyDown?: React.KeyboardEventHandler<HTMLInputElement>;
  onCurlImport?: (parsed: ParsedCurl) => void;
  placeholder?: string;
  className?: string;
}
```

- [ ] **Step 2: Destructure the new prop**

Find the destructuring:
```tsx
}: VariableAwareUrlInputProps) {
```

Look at the line above it — add `onCurlImport` to the destructured props:
```tsx
  onCurlImport,
```

- [ ] **Step 3: Add paste handler**

Find the `return (` statement. Add this handler BEFORE it:

```tsx
  const handlePaste = useCallback((e: React.ClipboardEvent<HTMLInputElement>) => {
    if (!onCurlImport) return;
    const text = e.clipboardData.getData('text/plain').trim();
    if (!/^curl\s/i.test(text)) return;
    e.preventDefault();
    const parsed = parseCurl(text);
    if (parsed) {
      onCurlImport(parsed);
    }
  }, [onCurlImport]);
```

- [ ] **Step 4: Wire the paste handler to the input**

Find the `<input` element:
```tsx
      <input
        ref={inputRef}
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={onKeyDown}
        placeholder={placeholder}
```

Add `onPaste={handlePaste}` after `onKeyDown`:
```tsx
      <input
        ref={inputRef}
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={onKeyDown}
        onPaste={handlePaste}
        placeholder={placeholder}
```

- [ ] **Step 5: Verify types**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -10
```

- [ ] **Step 6: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/components/request/VariableAwareUrlInput.tsx
git commit -m "feat: add cURL paste detection to VariableAwareUrlInput"
```

---

### Task 3: Wire cURL import in RequestPanel

**Files:**
- Modify: `src/components/request/RequestPanel.tsx`

**Depends on:** Task 2

- [ ] **Step 1: Add import**

Find the imports in `src/components/request/RequestPanel.tsx`. Add:

```tsx
import type { ParsedCurl } from '@/lib/curl-parser';
```

- [ ] **Step 2: Add the import handler**

Find the `handleParamsChange` callback (around line 125-132). Add AFTER it:

```tsx
  const handleCurlImport = useCallback((parsed: ParsedCurl) => {
    const patch: Partial<typeof request> = {
      method: (parsed.method as HttpMethod) || 'GET',
      url: parsed.url,
      headers: parsed.headers.map((h) => ({
        id: crypto.randomUUID(),
        key: h.key,
        value: h.value,
        enabled: true,
      })),
    };

    if (parsed.body) {
      patch.body = {
        mode: parsed.body.mode,
        content: parsed.body.content,
        formData: [],
      };
    }

    if (parsed.auth?.type === 'basic') {
      patch.auth = {
        authType: 'basic',
        basic: { username: parsed.auth.username, password: parsed.auth.password },
      };
    }

    // Sync query params from the parsed URL.
    patch.queryParams = parseQueryParams(parsed.url);

    updateRequest(tab.id, patch);
    setUrlError('');
    setCurlImported(true);
    setTimeout(() => setCurlImported(false), 3000);
  }, [tab.id, updateRequest]);
```

- [ ] **Step 3: Add curlImported state**

Find the state declarations (around line 69-71):
```tsx
  const [activeSection, setActiveSection] = useState<SectionTab>('params');
  const [unsavedDialogOpen, setUnsavedDialogOpen] = useState(false);
  const [urlError, setUrlError] = useState('');
```

Add after:
```tsx
  const [curlImported, setCurlImported] = useState(false);
```

- [ ] **Step 4: Pass onCurlImport to VariableAwareUrlInput**

Find:
```tsx
          <VariableAwareUrlInput
            value={request.url}
            onChange={(val) => { setUrlError(''); handleUrlChange(val); }}
            onKeyDown={(e) => { if (e.key === 'Enter') send(request); }}
            placeholder="https://api.example.com/resource"
          />
```

Replace with:
```tsx
          <VariableAwareUrlInput
            value={request.url}
            onChange={(val) => { setUrlError(''); handleUrlChange(val); }}
            onKeyDown={(e) => { if (e.key === 'Enter') send(request); }}
            onCurlImport={handleCurlImport}
            placeholder="https://api.example.com/resource"
          />
```

- [ ] **Step 5: Add import confirmation message**

Find the `{urlError && (` block (around line 185):
```tsx
        {urlError && (
          <p className="text-2xs text-destructive px-3 py-1">{urlError}</p>
        )}
```

Add AFTER it:
```tsx
        {curlImported && (
          <p className="text-2xs text-green-600 dark:text-green-400 px-3 py-1">Imported from cURL</p>
        )}
```

- [ ] **Step 6: Verify types and build**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -10
cd /home/numericlabs/data/Rust/Rocket && yarn build 2>&1 | tail -10
```

Expected: both succeed.

- [ ] **Step 7: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/components/request/RequestPanel.tsx
git commit -m "feat: wire cURL paste import into RequestPanel — populates method, URL, headers, body, auth"
```

---

## Done

cURL paste import complete:
- Pasting `curl -X POST https://api.example.com -H "Content-Type: application/json" -d '{"key": "value"}'` into the URL input auto-populates method (POST), URL, headers, body (JSON mode), and auth (if `-u` present)
- Multi-line cURL with backslash continuations supported
- Brief "Imported from cURL" confirmation shown for 3 seconds
- Normal paste (non-cURL text) works as before
