# cURL Paste Import — Design Spec

**Date:** 2026-03-26
**Branch:** feat/ux-workflows
**Goal:** Detect `curl` commands pasted into the URL input, parse them into structured requests (method, URL, headers, body, auth), and populate the request editor.

## Current State

- The URL input is `VariableAwareUrlInput` in `RequestPanel.tsx`
- Users can type or paste URLs directly
- No cURL detection or parsing exists
- The request state is managed via `usePaneStore.updateRequest(tabId, patch)`

## Feature

When a user pastes text starting with `curl ` (case-insensitive) into the URL field:

1. **Detect**: Intercept the paste event, check if the pasted text looks like a cURL command
2. **Parse**: Extract method, URL, headers, body, and auth from the cURL flags
3. **Populate**: Update the request editor with the parsed data (method, URL, headers, body, auth)
4. **Feedback**: Brief toast/inline message confirming "Imported from cURL"

### Supported cURL flags

| Flag | Maps to |
|---|---|
| `-X METHOD`, `--request METHOD` | `request.method` |
| `-H "Key: Value"`, `--header "Key: Value"` | `request.headers[]` |
| `-d "data"`, `--data "data"`, `--data-raw "data"` | `request.body` (JSON or text mode, auto-detected) |
| `-u user:pass`, `--user user:pass` | `request.auth` (basic auth) |
| `-A "agent"`, `--user-agent "agent"` | Header `User-Agent` |
| `--compressed` | Ignored (backend handles decompression) |
| `-k`, `--insecure` | Ignored (not configurable in UI) |
| `-L`, `--location` | Ignored (follows redirects by default) |

### Unsupported flags (silently ignored)

`-o`, `--output`, `-v`, `--verbose`, `-s`, `--silent`, `--cert`, `--key`, `--proxy`, `-F` (multipart form — future feature), `--cookie`

### Edge cases

- Multi-line cURL (backslash continuations): join lines before parsing
- Single and double quoted values: handle both
- URL without protocol: prepend `https://`
- Content-Type header: auto-set body mode (JSON/XML/text) based on value

## Architecture

### 1. cURL parser (`src/lib/curl-parser.ts`)

Pure function:
```ts
interface ParsedCurl {
  method: string;
  url: string;
  headers: { key: string; value: string }[];
  body?: { mode: 'json' | 'xml' | 'text'; content: string };
  auth?: { type: 'basic'; username: string; password: string };
}

function parseCurl(input: string): ParsedCurl | null
```

Returns `null` if the input is not a cURL command. Handles flag parsing, quoting, line continuations.

### 2. Paste detection in VariableAwareUrlInput

Add an `onPaste` handler to the real `<input>`. When pasted text starts with `curl ` (after trimming), call the parser and invoke a new `onCurlImport` callback prop instead of the normal `onChange`.

### 3. Integration in RequestPanel

Add `onCurlImport` handler that takes `ParsedCurl` and calls `updateRequest` with the full patch (method, url, headers, body, auth).

## Files Changed

| File | Changes |
|---|---|
| `src/lib/curl-parser.ts` | Create — cURL command parser |
| `src/components/request/VariableAwareUrlInput.tsx` | Add `onPaste` handler + `onCurlImport` callback prop |
| `src/components/request/RequestPanel.tsx` | Handle `onCurlImport` → update request state |

## Out of Scope

- Multipart form data (`-F`) parsing
- Cookie import (`--cookie`)
- Proxy configuration
- Certificate/key file handling
- cURL export (generating cURL from request)
