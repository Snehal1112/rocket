# shadcn/ui Standardization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace all hand-rolled UI patterns (custom checkbox buttons, plain labels, raw tab buttons) with their shadcn/ui equivalents app-wide so the UI is visually consistent and theme-correct in both light and dark mode.

**Architecture:** Pure presentational changes — no state, no data flow, no Tauri commands are altered. Work is split into seven focused tasks, each touching one or two files, each ending in a TypeScript check and commit.

**Tech Stack:** React 19 + TypeScript 5.8, shadcn/ui (Radix UI primitives), TailwindCSS 4.2, Yarn.

---

## Task 1: Fix `KeyValueEditor.tsx` — Checkbox + input heights

This component is already shared by params, query params, headers, and form-data editors. Fixing it here propagates the change to all four panels automatically.

**Files:**
- Modify: `src/components/request/KeyValueEditor.tsx`

- [ ] **Step 1: Replace the custom toggle Button with shadcn `Checkbox`**

  Open `src/components/request/KeyValueEditor.tsx` and replace the entire file with the following:

  ```tsx
  import { useCallback } from 'react';
  import { X, Plus } from 'lucide-react';
  import { Button } from '@/components/ui/button';
  import { Input } from '@/components/ui/input';
  import { Checkbox } from '@/components/ui/checkbox';
  import type { KeyValueEntry } from '@/types/pane-types';

  interface KeyValueEditorProps {
    entries: KeyValueEntry[];
    onChange: (entries: KeyValueEntry[]) => void;
    keyPlaceholder?: string;
    valuePlaceholder?: string;
    addLabel?: string;
    label?: string;
  }

  export function KeyValueEditor({
    entries,
    onChange,
    keyPlaceholder = 'Key',
    valuePlaceholder = 'Value',
    addLabel = 'Add Entry',
    label,
  }: KeyValueEditorProps) {
    const updateEntry = useCallback(
      (id: string, patch: Partial<KeyValueEntry>) => {
        onChange(entries.map((e) => (e.id === id ? { ...e, ...patch } : e)));
      },
      [entries, onChange],
    );

    const removeEntry = useCallback(
      (id: string) => {
        onChange(entries.filter((e) => e.id !== id));
      },
      [entries, onChange],
    );

    const addEntry = useCallback(() => {
      onChange([
        ...entries,
        { id: crypto.randomUUID(), key: '', value: '', enabled: true },
      ]);
    }, [entries, onChange]);

    return (
      <div className="space-y-2">
        {label && <div className="text-sm font-medium text-muted-foreground">{label}</div>}
        {entries.map((entry) => (
          <div key={entry.id} className="flex gap-2 items-center">
            <Checkbox
              checked={entry.enabled}
              onCheckedChange={(checked) => updateEntry(entry.id, { enabled: !!checked })}
              aria-label={`${entry.enabled ? 'Disable' : 'Enable'} ${entry.key || 'unnamed'}`}
            />
            <Input
              placeholder={keyPlaceholder}
              value={entry.key}
              onChange={(e) => updateEntry(entry.id, { key: e.target.value })}
              className="flex-1 text-xs"
            />
            <Input
              placeholder={valuePlaceholder}
              value={entry.value}
              onChange={(e) => updateEntry(entry.id, { value: e.target.value })}
              className="flex-1 text-xs"
            />
            <Button
              variant="ghost"
              size="icon"
              onClick={() => removeEntry(entry.id)}
              className="h-7 w-7"
              aria-label={`Remove ${entry.key || 'unnamed'}`}
            >
              <X className="h-4 w-4" />
            </Button>
          </div>
        ))}
        <Button variant="ghost" size="sm" onClick={addEntry} className="text-xs">
          <Plus className="h-3 w-3 mr-1" />
          {addLabel}
        </Button>
      </div>
    );
  }
  ```

- [ ] **Step 2: Verify TypeScript**

  ```bash
  yarn tsc --noEmit
  ```

  Expected: no errors.

- [ ] **Step 3: Commit**

  ```bash
  git add src/components/request/KeyValueEditor.tsx
  git commit -m "refactor(ui): replace custom checkbox button with shadcn Checkbox in KeyValueEditor"
  ```

---

## Task 2: Fix `EnvironmentDialog.tsx` — Checkbox + input heights

**Files:**
- Modify: `src/components/environments/EnvironmentDialog.tsx`

- [ ] **Step 1: Replace the toggle `Button` with `Checkbox` and remove `h-7` from `Input`s**

  In `src/components/environments/EnvironmentDialog.tsx`:

  **Change the import line** (line 3) — remove `Check` from lucide, add `Checkbox`:

  ```tsx
  import { Plus, Trash2, Eye, EyeOff, X } from 'lucide-react';
  import { Checkbox } from '@/components/ui/checkbox';
  ```

  **Replace the toggle button block** (lines 174–186):

  Before:
  ```tsx
  <Button
    variant="ghost"
    size="icon"
    onClick={() => updateVariable(idx, { enabled: !variable.enabled })}
    className={cn(
      'w-4 h-4 rounded border p-0 shrink-0',
      variable.enabled
        ? 'bg-primary border-primary text-primary-foreground hover:bg-primary/90'
        : 'border-gray-300 hover:bg-muted',
    )}
  >
    {variable.enabled && <Check className="h-3 w-3" />}
  </Button>
  ```

  After:
  ```tsx
  <Checkbox
    checked={variable.enabled}
    onCheckedChange={(checked) => updateVariable(idx, { enabled: !!checked })}
    aria-label={`${variable.enabled ? 'Disable' : 'Enable'} variable`}
  />
  ```

  **Remove `h-7` from both `Input` components** (lines 187–199):

  Before:
  ```tsx
  <Input
    placeholder="Key"
    value={variable.key}
    onChange={(e) => updateVariable(idx, { key: e.target.value })}
    className="flex-1 text-sm h-7"
  />
  <Input
    placeholder="Value"
    type={variable.secret ? 'password' : 'text'}
    value={variable.value}
    onChange={(e) => updateVariable(idx, { value: e.target.value })}
    className="flex-1 text-sm h-7"
  />
  ```

  After:
  ```tsx
  <Input
    placeholder="Key"
    value={variable.key}
    onChange={(e) => updateVariable(idx, { key: e.target.value })}
    className="flex-1 text-sm"
  />
  <Input
    placeholder="Value"
    type={variable.secret ? 'password' : 'text'}
    value={variable.value}
    onChange={(e) => updateVariable(idx, { value: e.target.value })}
    className="flex-1 text-sm"
  />
  ```

- [ ] **Step 2: Verify TypeScript**

  ```bash
  yarn tsc --noEmit
  ```

  Expected: no errors.

- [ ] **Step 3: Commit**

  ```bash
  git add src/components/environments/EnvironmentDialog.tsx
  git commit -m "refactor(ui): replace custom checkbox button with shadcn Checkbox in EnvironmentDialog"
  ```

---

## Task 3: Fix `CollectionVariablesEditor.tsx` — Checkbox + input heights

**Files:**
- Modify: `src/components/collections/CollectionVariablesEditor.tsx`

- [ ] **Step 1: Replace the toggle `Button` with `Checkbox` and remove `h-7` from `Input`s**

  In `src/components/collections/CollectionVariablesEditor.tsx`:

  **Change the import lines** (lines 1–2) — remove `Check` from lucide, add `Checkbox`:

  ```tsx
  import { useCallback } from 'react';
  import { Plus, X, Eye, EyeOff } from 'lucide-react';
  import { Button } from '@/components/ui/button';
  import { Input } from '@/components/ui/input';
  import { Checkbox } from '@/components/ui/checkbox';
  import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
  } from '@/components/ui/table';
  import type { CollectionVariable } from '@/lib/tauri-api';
  ```

  **Replace the toggle button block** (lines 74–86) inside the `<TableCell>`:

  Before:
  ```tsx
  <Button
    variant="ghost"
    size="icon"
    className={`w-4 h-4 rounded border p-0 ${
      v.enabled
        ? 'bg-primary border-primary text-primary-foreground hover:bg-primary/90'
        : 'border-gray-300 hover:bg-muted'
    }`}
    onClick={() => updateVar(i, { enabled: !v.enabled })}
    aria-label={v.enabled ? 'Disable variable' : 'Enable variable'}
  >
    {v.enabled && <Check className="h-3 w-3" />}
  </Button>
  ```

  After:
  ```tsx
  <Checkbox
    checked={v.enabled}
    onCheckedChange={(checked) => updateVar(i, { enabled: !!checked })}
    aria-label={v.enabled ? 'Disable variable' : 'Enable variable'}
  />
  ```

  **Remove `h-7` from all three `Input` components** (key, initialValue, value inputs):

  Before (each has `className="h-7 text-sm ..."`):
  ```tsx
  className="h-7 text-sm font-mono"
  // and
  className="h-7 text-sm"
  // and
  className="h-7 text-sm"
  ```

  After (remove `h-7` from each):
  ```tsx
  className="text-sm font-mono"
  // and
  className="text-sm"
  // and
  className="text-sm"
  ```

- [ ] **Step 2: Verify TypeScript**

  ```bash
  yarn tsc --noEmit
  ```

  Expected: no errors.

- [ ] **Step 3: Commit**

  ```bash
  git add src/components/collections/CollectionVariablesEditor.tsx
  git commit -m "refactor(ui): replace custom checkbox button with shadcn Checkbox in CollectionVariablesEditor"
  ```

---

## Task 4: Fix `RequestPanel.tsx` — Remove tab class overrides

**Files:**
- Modify: `src/components/request/RequestPanel.tsx`

- [ ] **Step 1: Remove custom class overrides from `TabsList` and all four `TabsTrigger`s**

  In `src/components/request/RequestPanel.tsx`, find the section starting at line 294.

  **Replace the entire `<TabsList>` opening tag** (line 294):

  Before:
  ```tsx
  <TabsList className="w-full justify-start rounded-none border-b border-border/70 bg-card/60 h-9 px-3">
  ```

  After:
  ```tsx
  <TabsList>
  ```

  **Replace the Params `TabsTrigger`** (lines 295–302):

  Before:
  ```tsx
  <TabsTrigger value="params" className="text-sm rounded-none data-[state=active]:border-b-2 data-[state=active]:border-primary data-[state=active]:bg-transparent">
    Params
    {enabledParamCount > 0 && (
      <span className="ml-1 text-2xs text-muted-foreground">
        ({enabledParamCount})
      </span>
    )}
  </TabsTrigger>
  ```

  After:
  ```tsx
  <TabsTrigger value="params">
    Params
    {enabledParamCount > 0 && (
      <span className="ml-1 text-2xs text-muted-foreground">
        ({enabledParamCount})
      </span>
    )}
  </TabsTrigger>
  ```

  **Replace the Headers `TabsTrigger`** (lines 303–310):

  Before:
  ```tsx
  <TabsTrigger value="headers" className="text-sm rounded-none data-[state=active]:border-b-2 data-[state=active]:border-primary data-[state=active]:bg-transparent">
    Headers
    {enabledHeaderCount > 0 && (
      <span className="ml-1 text-2xs text-muted-foreground">
        ({enabledHeaderCount})
      </span>
    )}
  </TabsTrigger>
  ```

  After:
  ```tsx
  <TabsTrigger value="headers">
    Headers
    {enabledHeaderCount > 0 && (
      <span className="ml-1 text-2xs text-muted-foreground">
        ({enabledHeaderCount})
      </span>
    )}
  </TabsTrigger>
  ```

  **Replace the Body `TabsTrigger`** (line 311–313):

  Before:
  ```tsx
  <TabsTrigger value="body" className="text-sm rounded-none data-[state=active]:border-b-2 data-[state=active]:border-primary data-[state=active]:bg-transparent">
    Body
  </TabsTrigger>
  ```

  After:
  ```tsx
  <TabsTrigger value="body">Body</TabsTrigger>
  ```

  **Replace the Auth `TabsTrigger`** (line 314–316):

  Before:
  ```tsx
  <TabsTrigger value="auth" className="text-sm rounded-none data-[state=active]:border-b-2 data-[state=active]:border-primary data-[state=active]:bg-transparent">
    Auth
  </TabsTrigger>
  ```

  After:
  ```tsx
  <TabsTrigger value="auth">Auth</TabsTrigger>
  ```

- [ ] **Step 2: Verify TypeScript**

  ```bash
  yarn tsc --noEmit
  ```

  Expected: no errors.

- [ ] **Step 3: Commit**

  ```bash
  git add src/components/request/RequestPanel.tsx
  git commit -m "refactor(ui): use shadcn default tab styling in RequestPanel"
  ```

---

## Task 5: Fix `ResponseBodyViewer.tsx` — Replace raw button tabs with shadcn `Tabs`

The current implementation uses raw `<button>` elements for the tab bar. Replace with `Tabs` + `TabsList` + `TabsTrigger`. Content is kept as conditional rendering (not `TabsContent`) to preserve the existing Monaco lazy-load behavior.

**Files:**
- Modify: `src/components/response/ResponseBodyViewer.tsx`

- [ ] **Step 1: Add `Tabs`, `TabsList`, `TabsTrigger` to imports**

  **Change line 3** (currently `import { Button } from '@/components/ui/button';`):

  Before:
  ```tsx
  import { Button } from '@/components/ui/button';
  ```

  After:
  ```tsx
  import { Button } from '@/components/ui/button';
  import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
  ```

- [ ] **Step 2: Replace the raw button tab bar**

  Find the tab bar block (lines 139–177). Replace this entire block:

  Before:
  ```tsx
  {/* Tab bar with copy button. */}
  <div className="flex items-center border-b border-border/70 px-1 shrink-0">
    <div className="flex h-8 flex-1 items-center gap-0">
      {(['pretty', 'raw', 'preview', 'headers'] as const).map((tab) => (
        <button
          key={tab}
          onClick={() => setActiveView(tab)}
          className={`h-7 px-3 text-sm capitalize transition-colors ${
            activeView === tab
              ? 'border-b-2 border-primary text-foreground font-medium'
              : 'text-muted-foreground hover:text-foreground'
          }`}
        >
          {tab}
          {tab === 'headers' && headerCount > 0 && (
            <span className="ml-1 text-2xs text-muted-foreground">
              ({headerCount})
            </span>
          )}
        </button>
      ))}
    </div>

    {/* Copy body button — visible on body tabs. */}
    {(activeView === 'pretty' || activeView === 'raw') && response.body && (
      <Button
        variant="ghost"
        size="icon"
        className="h-6 w-6 mr-1 shrink-0"
        title="Copy response body"
        onClick={handleCopyBody}
      >
        {copied ? (
          <Check className="h-3 w-3 text-emerald-500" />
        ) : (
          <Copy className="h-3 w-3" />
        )}
      </Button>
    )}
  </div>
  ```

  After:
  ```tsx
  {/* Tab bar with copy button. */}
  <div className="flex items-center border-b border-border/70 px-1 shrink-0">
    <Tabs
      value={activeView}
      onValueChange={(v) => setActiveView(v as ViewTab)}
      className="flex-1"
    >
      <TabsList>
        <TabsTrigger value="pretty">Pretty</TabsTrigger>
        <TabsTrigger value="raw">Raw</TabsTrigger>
        <TabsTrigger value="preview">Preview</TabsTrigger>
        <TabsTrigger value="headers">
          Headers
          {headerCount > 0 && (
            <span className="ml-1 text-2xs text-muted-foreground">
              ({headerCount})
            </span>
          )}
        </TabsTrigger>
      </TabsList>
    </Tabs>

    {/* Copy body button — visible on body tabs. */}
    {(activeView === 'pretty' || activeView === 'raw') && response.body && (
      <Button
        variant="ghost"
        size="icon"
        className="h-6 w-6 mr-1 shrink-0"
        title="Copy response body"
        onClick={handleCopyBody}
      >
        {copied ? (
          <Check className="h-3 w-3 text-emerald-500" />
        ) : (
          <Copy className="h-3 w-3" />
        )}
      </Button>
    )}
  </div>
  ```

- [ ] **Step 3: Verify TypeScript**

  ```bash
  yarn tsc --noEmit
  ```

  Expected: no errors.

- [ ] **Step 4: Commit**

  ```bash
  git add src/components/response/ResponseBodyViewer.tsx
  git commit -m "refactor(ui): replace raw button tabs with shadcn Tabs in ResponseBodyViewer"
  ```

---

## Task 6: Fix `AuthEditor.tsx` — Labels, raw checkbox, `<details>` element, input heights

This is the largest task. It covers: replacing 25+ `<label>` with `<Label>`, replacing the raw `<input type="checkbox">` with `<Checkbox>`, replacing `<details>`/`<summary>` with a `useState`-controlled toggle, and removing `h-8` from all `Input` components.

**Files:**
- Modify: `src/components/request/AuthEditor.tsx`

- [ ] **Step 1: Update imports**

  **Replace the existing import block** at the top of the file (lines 1–13):

  Before:
  ```tsx
  import { useCallback, useState } from 'react';
  import { User, Lock, Key } from 'lucide-react';
  import { Input } from '@/components/ui/input';
  import { Button } from '@/components/ui/button';
  import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
  } from '@/components/ui/select';
  import type { AuthState } from '@/types/pane-types';
  import { executeRequest, oauth2AuthCodeFlow } from '@/lib/tauri-api';
  ```

  After:
  ```tsx
  import { useCallback, useState } from 'react';
  import { User, Lock, Key, ChevronDown, ChevronRight } from 'lucide-react';
  import { Input } from '@/components/ui/input';
  import { Button } from '@/components/ui/button';
  import { Label } from '@/components/ui/label';
  import { Checkbox } from '@/components/ui/checkbox';
  import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
  } from '@/components/ui/select';
  import type { AuthState } from '@/types/pane-types';
  import { executeRequest, oauth2AuthCodeFlow } from '@/lib/tauri-api';
  ```

- [ ] **Step 2: Add `advancedOpen` state**

  In `AuthEditor` function body, find the existing state declarations (lines 102–103):

  ```tsx
  const [tokenError, setTokenError] = useState('');
  const [gettingToken, setGettingToken] = useState(false);
  ```

  Add `advancedOpen` after them:

  ```tsx
  const [tokenError, setTokenError] = useState('');
  const [gettingToken, setGettingToken] = useState(false);
  const [advancedOpen, setAdvancedOpen] = useState(false);
  ```

- [ ] **Step 3: Replace all `<label>` with `<Label>` and remove `h-8` from `Input`s in the OAuth2 section**

  Find the OAuth2 section (starting at line 388 with `{/* OAuth 2.0: labeled fields with small labels. */}`).

  Replace **every** occurrence of:
  ```tsx
  <label className="text-sm font-medium text-muted-foreground mb-1 block">...</label>
  ```
  with:
  ```tsx
  <Label className="mb-1">...</Label>
  ```

  At the same time, remove `h-8` from every `Input` in this section. Examples:

  Before:
  ```tsx
  <Input className="text-sm h-8 font-mono" placeholder="https://auth.example.com/authorize" ... />
  ```
  After:
  ```tsx
  <Input className="text-sm font-mono" placeholder="https://auth.example.com/authorize" ... />
  ```

  Before:
  ```tsx
  <Input className="text-sm h-8" placeholder="client-id" ... />
  ```
  After:
  ```tsx
  <Input className="text-sm" placeholder="client-id" ... />
  ```

  Also remove `h-8` from the `SelectTrigger` components inside this section:

  Before:
  ```tsx
  <SelectTrigger className="w-[200px] h-8 text-sm">
  ```
  After:
  ```tsx
  <SelectTrigger className="w-[200px] text-sm">
  ```

  Before:
  ```tsx
  <SelectTrigger className="w-full h-8 text-sm">
  ```
  After:
  ```tsx
  <SelectTrigger className="w-full text-sm">
  ```

- [ ] **Step 4: Replace `<details>`/`<summary>` with a `useState` toggle**

  Find the `<details>` block (lines 472–504):

  Before:
  ```tsx
  {/* Advanced Options — collapsible. */}
  <details className="text-sm">
    <summary className="cursor-pointer text-muted-foreground hover:text-foreground py-1">Advanced Options</summary>
    <div className="space-y-3 mt-2 pl-1">
      <div>
        <label className="text-sm font-medium text-muted-foreground mb-1 block">Client Authentication</label>
        <Select value={o.clientAuthentication} onValueChange={(v) => patchOAuth2({ clientAuthentication: v as 'header' | 'body' })}>
          <SelectTrigger className="w-full h-8 text-sm"><SelectValue /></SelectTrigger>
          <SelectContent>
            <SelectItem value="body" className="text-sm">Send in Request Body</SelectItem>
            <SelectItem value="header" className="text-sm">Send as Basic Auth Header</SelectItem>
          </SelectContent>
        </Select>
      </div>
      <div>
        <label className="text-sm font-medium text-muted-foreground mb-1 block">Header Prefix</label>
        <Input className="text-sm h-8" value={o.headerPrefix} onChange={(e) => patchOAuth2({ headerPrefix: e.target.value })} />
      </div>
      <div>
        <label className="text-sm font-medium text-muted-foreground mb-1 block">Add Token To</label>
        <Select value={o.addTokenTo} onValueChange={(v) => patchOAuth2({ addTokenTo: v as 'header' | 'queryParams' })}>
          <SelectTrigger className="w-full h-8 text-sm"><SelectValue /></SelectTrigger>
          <SelectContent>
            <SelectItem value="header" className="text-sm">Header</SelectItem>
            <SelectItem value="queryParams" className="text-sm">Query Params</SelectItem>
          </SelectContent>
        </Select>
      </div>
      <label className="flex items-center gap-2 cursor-pointer">
        <input type="checkbox" checked={o.verifySsl} onChange={(e) => patchOAuth2({ verifySsl: e.target.checked })} className="rounded" />
        <span className="text-xs text-muted-foreground">Verify SSL certificates</span>
      </label>
    </div>
  </details>
  ```

  After:
  ```tsx
  {/* Advanced Options — collapsible. */}
  <div className="text-sm">
    <button
      type="button"
      className="flex items-center gap-1 cursor-pointer text-muted-foreground hover:text-foreground py-1"
      onClick={() => setAdvancedOpen(!advancedOpen)}
    >
      {advancedOpen ? (
        <ChevronDown className="h-3 w-3" />
      ) : (
        <ChevronRight className="h-3 w-3" />
      )}
      Advanced Options
    </button>
    {advancedOpen && (
      <div className="space-y-3 mt-2 pl-1">
        <div>
          <Label className="mb-1">Client Authentication</Label>
          <Select value={o.clientAuthentication} onValueChange={(v) => patchOAuth2({ clientAuthentication: v as 'header' | 'body' })}>
            <SelectTrigger className="w-full text-sm"><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="body" className="text-sm">Send in Request Body</SelectItem>
              <SelectItem value="header" className="text-sm">Send as Basic Auth Header</SelectItem>
            </SelectContent>
          </Select>
        </div>
        <div>
          <Label className="mb-1">Header Prefix</Label>
          <Input className="text-sm" value={o.headerPrefix} onChange={(e) => patchOAuth2({ headerPrefix: e.target.value })} />
        </div>
        <div>
          <Label className="mb-1">Add Token To</Label>
          <Select value={o.addTokenTo} onValueChange={(v) => patchOAuth2({ addTokenTo: v as 'header' | 'queryParams' })}>
            <SelectTrigger className="w-full text-sm"><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="header" className="text-sm">Header</SelectItem>
              <SelectItem value="queryParams" className="text-sm">Query Params</SelectItem>
            </SelectContent>
          </Select>
        </div>
        <div className="flex items-center gap-2">
          <Checkbox
            id="verify-ssl"
            checked={o.verifySsl}
            onCheckedChange={(checked) => patchOAuth2({ verifySsl: !!checked })}
          />
          <Label htmlFor="verify-ssl" className="text-xs text-muted-foreground cursor-pointer">
            Verify SSL certificates
          </Label>
        </div>
      </div>
    )}
  </div>
  ```

- [ ] **Step 5: Replace `<label>` with `<Label>` and remove `h-8` from the Token section**

  In the token section (lines 506–535), replace:

  Before:
  ```tsx
  <label className="text-sm font-medium text-muted-foreground mb-1 block">Access Token</label>
  <div className="flex gap-1.5">
    <Input className="h-8 flex-1 text-sm truncate" readOnly value={o.accessToken} placeholder="(none)" title={o.accessToken || undefined} />
    <Button variant="outline" size="sm" className="h-8 px-2 text-sm shrink-0" onClick={() => navigator.clipboard.writeText(o.accessToken)} title="Copy">Copy</Button>
  </div>
  ```

  After:
  ```tsx
  <Label className="mb-1">Access Token</Label>
  <div className="flex gap-1.5">
    <Input className="flex-1 text-sm truncate" readOnly value={o.accessToken} placeholder="(none)" title={o.accessToken || undefined} />
    <Button variant="outline" size="sm" className="px-2 text-sm shrink-0" onClick={() => navigator.clipboard.writeText(o.accessToken)} title="Copy">Copy</Button>
  </div>
  ```

  And if `o.refreshToken` is present:

  Before:
  ```tsx
  <label className="text-sm font-medium text-muted-foreground mb-1 block">Refresh Token</label>
  <div className="flex gap-1.5">
    <Input className="h-8 flex-1 text-sm truncate" readOnly value={o.refreshToken} />
    <Button variant="outline" size="sm" className="h-8 px-2 text-sm shrink-0" onClick={() => navigator.clipboard.writeText(o.refreshToken)} title="Copy">Copy</Button>
  </div>
  ```

  After:
  ```tsx
  <Label className="mb-1">Refresh Token</Label>
  <div className="flex gap-1.5">
    <Input className="flex-1 text-sm truncate" readOnly value={o.refreshToken} />
    <Button variant="outline" size="sm" className="px-2 text-sm shrink-0" onClick={() => navigator.clipboard.writeText(o.refreshToken)} title="Copy">Copy</Button>
  </div>
  ```

  Also remove `h-8` from the Get Token / Refresh buttons:

  Before:
  ```tsx
  <Button variant="outline" size="sm" className="h-8 text-sm" ...>
  ```
  After:
  ```tsx
  <Button variant="outline" size="sm" className="text-sm" ...>
  ```

- [ ] **Step 6: Replace `<label>` with `<Label>` and remove `h-8` from the AWS SigV4 section**

  Find the AWS Signature v4 panel (line 541 onward). Replace every `<label className="text-sm font-medium text-muted-foreground mb-1 block">...</label>` with `<Label className="mb-1">...</Label>` and remove `h-8` from every `Input`:

  The five labels are: Access Key, Secret Key, Region, Service, Session Token.

  Before (each):
  ```tsx
  <label className="text-sm font-medium text-muted-foreground mb-1 block">
    Access Key
  </label>
  <Input
    className="text-sm h-8"
    ...
  />
  ```

  After (each):
  ```tsx
  <Label className="mb-1">Access Key</Label>
  <Input
    className="text-sm"
    ...
  />
  ```

  Apply the same pattern for Secret Key, Region, Service, and Session Token inputs.

- [ ] **Step 7: Verify TypeScript**

  ```bash
  yarn tsc --noEmit
  ```

  Expected: no errors.

- [ ] **Step 8: Commit**

  ```bash
  git add src/components/request/AuthEditor.tsx
  git commit -m "refactor(ui): replace plain labels, raw checkbox, and details element with shadcn components in AuthEditor"
  ```

---

## Task 7: Final validation

- [ ] **Step 1: Run TypeScript check**

  ```bash
  yarn tsc --noEmit
  ```

  Expected: 0 errors.

- [ ] **Step 2: Run frontend build**

  ```bash
  yarn build
  ```

  Expected: build succeeds with no errors.

- [ ] **Step 3: Manual smoke test checklist**

  Launch the app with `yarn tauri dev` and verify each of the following:

  - [ ] Params, headers, query params tabs: checkboxes render correctly, toggling enables/disables a row, checked state is visible in both light and dark mode (no grey border artifact in dark mode).
  - [ ] Environment dialog: variable checkboxes behave the same way.
  - [ ] Collection variables editor: variable checkboxes behave the same way.
  - [ ] Request panel tabs (Params / Headers / Body / Auth): active tab shows the shadcn background + shadow pill style, not an underline.
  - [ ] Response viewer tabs (Pretty / Raw / Preview / Headers): same background + shadow active state.
  - [ ] Auth form — OAuth2 section: labels are full-contrast (not muted), inputs are visibly taller (h-9), Advanced Options toggle shows chevron and expands/collapses the section.
  - [ ] Auth form — OAuth2 Advanced: "Verify SSL certificates" renders as a shadcn checkbox with a label.
  - [ ] Auth form — AWS SigV4 section: labels are full-contrast, inputs are h-9.
  - [ ] Form-data body mode: KeyValueEditor rows use taller inputs.
