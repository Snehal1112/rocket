# Post Response Snippet Sidebar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a toggleable snippet sidebar to the Post Response tab in ScriptsTab, giving users quick access to common post-processing patterns and the res.*/rok.* API reference.

**Architecture:** Add a `POST_RESPONSE_SNIPPETS` data constant alongside the existing `ROK_SNIPPETS` in `rok-types.ts`, make `ScriptSnippetSidebar` accept an optional `snippets` prop so it can render either dataset, then wire a toggle button + conditional sidebar into the Post Response tab in `ScriptsTab.tsx`. No new files needed.

**Tech Stack:** React, TypeScript, Zustand (not used here), shadcn/ui (`Button`), lucide-react (`PanelRight` icon), existing `ScriptSnippetSidebar` component.

---

### Task 1: Add `POST_RESPONSE_SNIPPETS` to `rok-types.ts`

**Files:**
- Modify: `src/components/editor/rok-types.ts`

- [ ] **Step 1: Add the new snippet constant after the closing bracket of `ROK_SNIPPETS`**

In `src/components/editor/rok-types.ts`, after line 208 (the closing `];` of `ROK_SNIPPETS`), insert:

```typescript
export const POST_RESPONSE_SNIPPETS: ScriptSnippetGroup[] = [
  {
    id: 'common-patterns',
    label: 'Common Patterns',
    items: [
      {
        label: 'Save body field to env var',
        kind: 'template',
        code: `const value = res.getBody().field;\nrok.setEnvVar("key", value);`,
      },
      {
        label: 'Save header to env var',
        kind: 'template',
        code: `const value = res.getHeader("header-name");\nrok.setEnvVar("key", value);`,
      },
      {
        label: 'Log response body',
        kind: 'template',
        code: `console.log(res.getBody());`,
      },
      {
        label: 'Set collection var from body',
        kind: 'template',
        code: `const value = res.getBody().field;\nrok.setCollectionVar("key", value);`,
      },
      {
        label: 'Set var only if 2xx',
        kind: 'template',
        code: `if (res.getStatus() >= 200 && res.getStatus() < 300) {\n  rok.setEnvVar("key", res.getBody().field);\n}`,
      },
    ],
  },
  {
    id: 'api-reference',
    label: 'API Reference',
    subGroups: [
      {
        id: 'res',
        label: 'res.*',
        items: [
          { label: 'res.getStatus()', kind: 'expression', code: 'res.getStatus()' },
          { label: 'res.getStatusText()', kind: 'expression', code: 'res.getStatusText()' },
          { label: 'res.getHeader("name")', kind: 'expression', code: 'res.getHeader("name")' },
          { label: 'res.getHeaders()', kind: 'expression', code: 'res.getHeaders()' },
          { label: 'res.getBody()', kind: 'expression', code: 'res.getBody()' },
          {
            label: 'res.getBody({ raw: true })',
            kind: 'expression',
            code: 'res.getBody({ raw: true })',
          },
          { label: 'res.getResponseTime()', kind: 'expression', code: 'res.getResponseTime()' },
        ],
      },
      {
        id: 'rok',
        label: 'rok.*',
        items: [
          { label: 'rok.getVar("key")', kind: 'expression', code: 'rok.getVar("key")' },
          {
            label: 'rok.setVar("key", value)',
            kind: 'expression',
            code: 'rok.setVar("key", value)',
          },
          { label: 'rok.getEnvVar("key")', kind: 'expression', code: 'rok.getEnvVar("key")' },
          {
            label: 'rok.setEnvVar("key", value)',
            kind: 'expression',
            code: 'rok.setEnvVar("key", value)',
          },
          {
            label: 'rok.getCollectionVar("key")',
            kind: 'expression',
            code: 'rok.getCollectionVar("key")',
          },
          { label: 'rok.getEnvName()', kind: 'expression', code: 'rok.getEnvName()' },
          {
            label: 'rok.interpolate("{{template}}")',
            kind: 'expression',
            code: 'rok.interpolate("{{template}}")',
          },
        ],
      },
    ],
  },
];
```

- [ ] **Step 2: Verify TypeScript is happy**

```bash
yarn tsc --noEmit 2>&1 | head -20
```

Expected: no errors relating to `rok-types.ts`.

- [ ] **Step 3: Commit**

```bash
git add src/components/editor/rok-types.ts
git commit -m "feat: add POST_RESPONSE_SNIPPETS to rok-types"
```

---

### Task 2: Make `ScriptSnippetSidebar` accept an optional `snippets` prop

**Files:**
- Modify: `src/components/request/ScriptSnippetSidebar.tsx`

- [ ] **Step 1: Update the props interface and component signature**

Replace the current interface and component signature (lines 12–14 and 98):

```typescript
// Before
interface ScriptSnippetSidebarProps {
  onInsert: (code: string) => void;
}
// ...
export function ScriptSnippetSidebar({ onInsert }: ScriptSnippetSidebarProps) {
```

```typescript
// After
interface ScriptSnippetSidebarProps {
  onInsert: (code: string) => void;
  snippets?: ScriptSnippetGroup[];
}
// ...
export function ScriptSnippetSidebar({ onInsert, snippets = ROK_SNIPPETS }: ScriptSnippetSidebarProps) {
```

- [ ] **Step 2: Update the render loop to use the `snippets` prop instead of the hard-coded import**

Replace line 152:

```typescript
// Before
{ROK_SNIPPETS.map((group) => (
```

```typescript
// After
{snippets.map((group) => (
```

- [ ] **Step 3: Verify TypeScript is happy**

```bash
yarn tsc --noEmit 2>&1 | head -20
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/components/request/ScriptSnippetSidebar.tsx
git commit -m "feat: accept optional snippets prop in ScriptSnippetSidebar"
```

---

### Task 3: Wire toggle button and sidebar into Post Response tab

**Files:**
- Modify: `src/components/request/ScriptsTab.tsx`

- [ ] **Step 1: Add imports**

At the top of `src/components/request/ScriptsTab.tsx`, add `useState` to the React import and add two new named imports:

```typescript
// Change line 2 from:
import { lazy, Suspense, useCallback, useRef } from 'react';
// To:
import { lazy, Suspense, useCallback, useRef, useState } from 'react';
```

Add after the existing import on line 4:

```typescript
import { PanelRight } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { POST_RESPONSE_SNIPPETS } from '@/components/editor/rok-types';
```

- [ ] **Step 2: Add the toggle state**

Inside the `ScriptsTab` function body, after the `handleInsert` callback (after line 73), add:

```typescript
const [showPostResponseSidebar, setShowPostResponseSidebar] = useState(false);
```

- [ ] **Step 3: Add the toggle button to the tab bar**

The current `TabsList` (line 77) ends with the Tests trigger. Add the Snippets toggle button after the Tests trigger and before the closing `</TabsList>`:

```tsx
<TabsList className='shrink-0 w-full justify-start rounded-none border-b bg-transparent px-2'>
  <TabsTrigger value='pre-request' className='text-xs'>
    Pre Request
  </TabsTrigger>
  <TabsTrigger value='post-response' className='text-xs'>
    Post Response
  </TabsTrigger>
  <TabsTrigger value='tests' className='text-xs'>
    Tests
  </TabsTrigger>
  <div className='ml-auto flex items-center'>
    <Tabs.TabsConsumer>
      {/* Rendered via conditional below — see Step 4 */}
    </Tabs.TabsConsumer>
  </div>
</TabsList>
```

Wait — shadcn `Tabs` doesn't expose a consumer. Instead, track the active tab with a `value` + `onValueChange` on the `<Tabs>` root so we know when Post Response is active. Replace line 76:

```tsx
// Before
<Tabs defaultValue='pre-request' className='flex flex-col h-full'>
```

```tsx
// After
const [activeTab, setActiveTab] = useState('pre-request');

// In JSX:
<Tabs value={activeTab} onValueChange={setActiveTab} className='flex flex-col h-full'>
```

Then add the Snippets button inside `TabsList`, after the Tests trigger:

```tsx
{activeTab === 'post-response' && (
  <Button
    variant='ghost'
    size='sm'
    className='ml-auto h-7 gap-1 text-xs'
    onClick={() => setShowPostResponseSidebar((v) => !v)}
  >
    <PanelRight className='h-3.5 w-3.5' />
    Snippets
  </Button>
)}
```

- [ ] **Step 4: Add `onEditorReady` to the Post Response editor and render the sidebar**

Replace the current Post Response `TabsContent` (lines 102–113):

```tsx
// Before
<TabsContent value='post-response' className='flex-1 m-0 p-0'>
  <Suspense fallback={null}>
    <MonacoWrapper
      language='javascript'
      value={postResponseScript}
      onChange={readOnly ? undefined : onChangePostResponse}
      readOnly={readOnly}
      height='100%'
      phase='post-response'
    />
  </Suspense>
</TabsContent>
```

```tsx
// After
<TabsContent value='post-response' className='flex-1 m-0 p-0 flex overflow-hidden'>
  <div className='flex-1 min-w-0'>
    <Suspense fallback={null}>
      <MonacoWrapper
        language='javascript'
        value={postResponseScript}
        onChange={readOnly ? undefined : onChangePostResponse}
        readOnly={readOnly}
        height='100%'
        phase='post-response'
        onEditorReady={handleEditorReady}
      />
    </Suspense>
  </div>
  {!readOnly && showPostResponseSidebar && (
    <ScriptSnippetSidebar snippets={POST_RESPONSE_SNIPPETS} onInsert={handleInsert} />
  )}
</TabsContent>
```

- [ ] **Step 5: Verify TypeScript is happy**

```bash
yarn tsc --noEmit 2>&1 | head -30
```

Expected: no errors.

- [ ] **Step 6: Run Biome lint**

```bash
yarn check 2>&1 | head -30
```

Expected: no errors. If Biome flags unused imports or ordering, run `yarn format` to auto-fix, then re-run `yarn check`.

- [ ] **Step 7: Commit**

```bash
git add src/components/request/ScriptsTab.tsx
git commit -m "feat: add snippet sidebar toggle to Post Response tab"
```

---

### Task 4: Manual smoke test

- [ ] **Step 1: Start the dev server**

```bash
yarn tauri dev
```

- [ ] **Step 2: Open any request and navigate to the Scripts tab → Post Response**

Verify: no "Snippets" button visible on Pre Request or Tests tab. A "Snippets" button (with panel icon) appears only on Post Response.

- [ ] **Step 3: Click the Snippets button**

Verify: sidebar slides in to the right of the editor with two groups — "Common Patterns" and "API Reference" (with `res.*` and `rok.*` sub-groups, no `expect` group).

- [ ] **Step 4: Click a snippet**

Click "Save body field to env var". Verify it inserts at the cursor:

```javascript
const value = res.getBody().field;
rok.setEnvVar("key", value);
```

- [ ] **Step 5: Switch to Tests tab**

Verify: Tests sidebar is unaffected (still always visible, still shows "Common Tests" and "API Reference" with `expect`).

- [ ] **Step 6: Switch back to Post Response**

Verify: sidebar is closed again (state resets on tab switch because `showPostResponseSidebar` is a local `useState` that does not persist).

- [ ] **Step 7: Confirm Pre Request is unchanged**

Navigate to Pre Request tab. Verify: no sidebar, no toggle button.
