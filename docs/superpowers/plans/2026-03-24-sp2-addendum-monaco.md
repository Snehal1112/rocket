# SP2 Addendum: Monaco Editor Integration

> This addendum modifies SP2 Plans 1 and 5. Insert these tasks at the specified positions.

## Overview

Use `@monaco-editor/react` for request body editing and response body viewing. A shared wrapper component handles initialization, theme syncing (light/dark mode), language detection, and common config.

**Package:** `@monaco-editor/react` (React wrapper for Monaco)

**Where Monaco is used:**
- Request body editor: JSON, XML, text, GraphQL (read-write)
- Response body viewer: JSON, XML, HTML, text (read-only, with pretty-print)
- Pre/post scripts editor: JavaScript (SP3, future — just ensure the wrapper supports it)

**Where Monaco is NOT used:**
- URL bar (plain input)
- Key-value tables (headers, params, form-data — shadcn table inputs)
- Environment variable editor (shadcn form)
- Auth config forms (shadcn inputs)

---

## Insert into SP2 Plan 1 — after Task 3 (pane store), before Task 4 (PaneRenderer)

### Task 3.5: Monaco editor wrapper component

**Files:**
- Create: `frontend/src/components/editor/MonacoWrapper.tsx`
- Create: `frontend/src/components/editor/monaco-config.ts`
- Create: `frontend/src/components/editor/useMonacoTheme.ts`

- [ ] **Step 1: Install Monaco**

```bash
cd frontend && npm install @monaco-editor/react
```

- [ ] **Step 2: Create Monaco configuration**

`frontend/src/components/editor/monaco-config.ts`:
```typescript
import type { editor } from 'monaco-editor';

export const SHARED_EDITOR_OPTIONS: editor.IStandaloneEditorConstructionOptions = {
  fontSize: 13,
  fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace",
  lineNumbers: 'on',
  minimap: { enabled: false },
  scrollBeyondLastLine: false,
  automaticLayout: true,
  tabSize: 2,
  wordWrap: 'on',
  renderWhitespace: 'selection',
  bracketPairColorization: { enabled: true },
  padding: { top: 8, bottom: 8 },
  scrollbar: {
    verticalScrollbarSize: 8,
    horizontalScrollbarSize: 8,
  },
};

export const READONLY_OPTIONS: editor.IStandaloneEditorConstructionOptions = {
  ...SHARED_EDITOR_OPTIONS,
  readOnly: true,
  domReadOnly: true,
  lineNumbers: 'on',
  minimap: { enabled: true, maxColumn: 80 },
  renderValidationDecorations: 'off',
};

// Map body mode / content-type to Monaco language ID
export function detectLanguage(mode?: string, contentType?: string): string {
  if (mode) {
    switch (mode) {
      case 'json': return 'json';
      case 'xml': return 'xml';
      case 'text': return 'plaintext';
      case 'graphql': return 'graphql';
      case 'javascript': return 'javascript';
      default: return 'plaintext';
    }
  }
  if (contentType) {
    if (contentType.includes('json')) return 'json';
    if (contentType.includes('xml')) return 'xml';
    if (contentType.includes('html')) return 'html';
    if (contentType.includes('javascript')) return 'javascript';
    if (contentType.includes('css')) return 'css';
  }
  return 'plaintext';
}
```

- [ ] **Step 3: Create theme sync hook**

`frontend/src/components/editor/useMonacoTheme.ts`:
```typescript
import { useEffect, useState } from 'react';
import { useMonaco } from '@monaco-editor/react';

// Custom themes that match the app's design system
const LIGHT_THEME = {
  base: 'vs' as const,
  inherit: true,
  rules: [
    { token: 'string', foreground: '0F6E56' },    // teal-600
    { token: 'number', foreground: 'BA7517' },     // amber-400
    { token: 'keyword', foreground: '534AB7' },    // purple-600
    { token: 'comment', foreground: '888780' },    // gray-400
    { token: 'type', foreground: '185FA5' },       // blue-600
  ],
  colors: {
    'editor.background': '#FFFFFF',
    'editor.foreground': '#2C2C2A',
    'editor.lineHighlightBackground': '#F1EFE8',
    'editorLineNumber.foreground': '#B4B2A9',
    'editorIndentGuide.background': '#D3D1C7',
    'editor.selectionBackground': '#B5D4F4',
  },
};

const DARK_THEME = {
  base: 'vs-dark' as const,
  inherit: true,
  rules: [
    { token: 'string', foreground: '5DCAA5' },    // teal-200
    { token: 'number', foreground: 'FAC775' },     // amber-100
    { token: 'keyword', foreground: 'AFA9EC' },    // purple-200
    { token: 'comment', foreground: '888780' },    // gray-400
    { token: 'type', foreground: '85B7EB' },       // blue-200
  ],
  colors: {
    'editor.background': '#1a1a19',
    'editor.foreground': '#D3D1C7',
    'editor.lineHighlightBackground': '#2C2C2A',
    'editorLineNumber.foreground': '#5F5E5A',
    'editorIndentGuide.background': '#444441',
    'editor.selectionBackground': '#0C447C',
  },
};

export function useMonacoTheme() {
  const monaco = useMonaco();
  const [isDark, setIsDark] = useState(
    window.matchMedia('(prefers-color-scheme: dark)').matches,
  );

  useEffect(() => {
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const handler = (e: MediaQueryListEvent) => setIsDark(e.matches);
    mq.addEventListener('change', handler);
    return () => mq.removeEventListener('change', handler);
  }, []);

  useEffect(() => {
    if (!monaco) return;
    monaco.editor.defineTheme('rocket-light', LIGHT_THEME);
    monaco.editor.defineTheme('rocket-dark', DARK_THEME);
    monaco.editor.setTheme(isDark ? 'rocket-dark' : 'rocket-light');
  }, [monaco, isDark]);

  return isDark ? 'rocket-dark' : 'rocket-light';
}
```

- [ ] **Step 4: Create MonacoWrapper component**

`frontend/src/components/editor/MonacoWrapper.tsx`:
```tsx
import Editor, { type OnMount, type OnChange } from '@monaco-editor/react';
import { useMonacoTheme } from './useMonacoTheme';
import { SHARED_EDITOR_OPTIONS, READONLY_OPTIONS, detectLanguage } from './monaco-config';

interface MonacoWrapperProps {
  value: string;
  onChange?: (value: string) => void;
  language?: string;       // explicit language override
  bodyMode?: string;       // auto-detect from body mode
  contentType?: string;    // auto-detect from Content-Type header
  readOnly?: boolean;
  height?: string | number;
  onMount?: OnMount;
}

export function MonacoWrapper({
  value,
  onChange,
  language,
  bodyMode,
  contentType,
  readOnly = false,
  height = '300px',
  onMount,
}: MonacoWrapperProps) {
  const theme = useMonacoTheme();
  const resolvedLanguage = language ?? detectLanguage(bodyMode, contentType);
  const options = readOnly ? READONLY_OPTIONS : SHARED_EDITOR_OPTIONS;

  const handleChange: OnChange = (val) => {
    if (onChange && val !== undefined) {
      onChange(val);
    }
  };

  return (
    <Editor
      height={height}
      language={resolvedLanguage}
      value={value}
      onChange={handleChange}
      theme={theme}
      options={options}
      onMount={onMount}
      loading={
        <div style={{
          height: typeof height === 'number' ? `${height}px` : height,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          color: 'var(--color-text-tertiary)',
          fontSize: '13px',
        }}>
          Loading editor...
        </div>
      }
    />
  );
}
```

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/editor/
git commit -m "feat(editor): Monaco wrapper with theme sync + language detection"
```

---

## Modify SP2 Plan 1 Task 4 — EditorGroup uses Monaco for body

### In EditorGroup.tsx, the request body section:

Replace the existing `<textarea>` or body editor with:
```tsx
import { MonacoWrapper } from '@/components/editor/MonacoWrapper';

// Inside the Body tab of the request panel:
{bodyState.mode !== 'none' && bodyState.mode !== 'formdata' && bodyState.mode !== 'binary' && (
  <MonacoWrapper
    value={bodyState.content}
    onChange={(val) => updateRequest(activeTab.id, {
      body: { ...bodyState, content: val },
    })}
    bodyMode={bodyState.mode}
    height="250px"
  />
)}
```

The body mode selector (JSON, XML, text, etc.) sits above the editor and switches Monaco's language automatically via the `bodyMode` prop.

---

## Modify SP2 Plan 5 Task 2 — Response body uses Monaco read-only

### Replace the regex tokenizer approach with Monaco:

The original plan called for a "lightweight regex-based tokenizer" for pretty-printing. With Monaco, this becomes much simpler:

```tsx
// ResponseBodyViewer.tsx — Pretty view
import { MonacoWrapper } from '@/components/editor/MonacoWrapper';

function PrettyView({ body, contentType }: { body: string; contentType?: string }) {
  // Auto-format JSON
  let formatted = body;
  if (contentType?.includes('json')) {
    try {
      formatted = JSON.stringify(JSON.parse(body), null, 2);
    } catch {
      // Not valid JSON — show raw
    }
  }

  return (
    <MonacoWrapper
      value={formatted}
      readOnly
      contentType={contentType}
      height="100%"
    />
  );
}

function RawView({ body }: { body: string }) {
  return (
    <MonacoWrapper
      value={body}
      readOnly
      language="plaintext"
      height="100%"
    />
  );
}

// HTML preview remains an iframe (not Monaco):
function PreviewView({ body }: { body: string }) {
  return <iframe srcDoc={body} sandbox="" style={{ width: '100%', height: '100%', border: 'none' }} />;
}
```

---

## Performance notes for implementers

- **Monaco is heavy (~2MB).** Use `@monaco-editor/react`'s lazy loading — it loads Monaco from CDN by default, which is fine for a desktop app. For production builds, configure the webpack/vite Monaco plugin to bundle workers locally.

- **One Monaco instance per visible tab.** Don't mount Monaco for background tabs — only the active tab in each editor group renders its Monaco instance. Use a key prop to force remount when switching tabs to avoid stale state.

- **Tauri CSP:** The default Tauri CSP may block Monaco's CDN worker loading. If Monaco fails to initialize, add to `tauri.conf.json`:
  ```json
  "app": {
    "security": {
      "csp": "default-src 'self'; script-src 'self' 'unsafe-eval' https://cdn.jsdelivr.net; style-src 'self' 'unsafe-inline'; worker-src blob:;"
    }
  }
  ```
  Or use the `@monaco-editor/react` loader config to point to local workers:
  ```typescript
  import { loader } from '@monaco-editor/react';
  loader.config({ paths: { vs: '/monaco-editor/min/vs' } });
  ```

- **For SP3 (scripting):** The MonacoWrapper already supports `language="javascript"`. When building the script editor, just pass `language="javascript"` and it works. No additional Monaco setup needed.
