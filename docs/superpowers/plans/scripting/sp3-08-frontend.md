# SP3-08 — Frontend: Scripts Tab, Vars Tab, Tests Panel, Console Wiring

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the Scripts tab (Monaco, 3 sub-tabs), Vars tab (declarative key/value tables), Tests panel in the response area, and wire console entries from `ExecuteRequestResponse` to the existing Console tab in the status bar.

**Architecture:** All UI uses shadcn/ui primitives only — no raw HTML elements. Scripts and Vars tabs are added to the existing `RequestEditor` tab strip. The Tests panel is a new tab in `ResponsePanel`. `consoleStore` gains a handler for the new `ConsoleOutput` domain event. Monaco is used for all script editors (multi-line).

**Tech Stack:** React 18, TypeScript, shadcn/ui, Monaco Editor, Zustand, Lucide React

**Spec:** `docs/superpowers/specs/2026-05-20-sp3-js-scripting-design.md` §7

**Depends on:** SP3-07 merged.

---

## Task 1: `ScriptsTab` component (Monaco, 3 sub-tabs)

**Files:**
- Create: `frontend/src/components/request/ScriptsTab.tsx`
- Modify: `frontend/src/components/request/RequestEditor.tsx`

- [ ] **Step 1: Create `frontend/src/components/request/ScriptsTab.tsx`**

```tsx
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { MonacoEditor } from '@/components/editor/MonacoEditor'

interface ScriptsTabProps {
  preRequestScript: string
  postResponseScript: string
  testsScript: string
  onChangePreRequest: (value: string) => void
  onChangePostResponse: (value: string) => void
  onChangeTests: (value: string) => void
  readOnly?: boolean
}

export function ScriptsTab({
  preRequestScript,
  postResponseScript,
  testsScript,
  onChangePreRequest,
  onChangePostResponse,
  onChangeTests,
  readOnly = false,
}: ScriptsTabProps) {
  return (
    <Tabs defaultValue="pre-request" className="flex flex-col h-full">
      <TabsList className="shrink-0 w-full justify-start rounded-none border-b bg-transparent px-2">
        <TabsTrigger value="pre-request" className="text-xs">
          Pre Request
        </TabsTrigger>
        <TabsTrigger value="post-response" className="text-xs">
          Post Response
        </TabsTrigger>
        <TabsTrigger value="tests" className="text-xs">
          Tests
        </TabsTrigger>
      </TabsList>

      <TabsContent value="pre-request" className="flex-1 m-0 p-0">
        <MonacoEditor
          language="javascript"
          value={preRequestScript}
          onChange={readOnly ? undefined : onChangePreRequest}
          options={{ readOnly, minimap: { enabled: false }, lineNumbers: 'on' }}
          className="h-full"
        />
      </TabsContent>

      <TabsContent value="post-response" className="flex-1 m-0 p-0">
        <MonacoEditor
          language="javascript"
          value={postResponseScript}
          onChange={readOnly ? undefined : onChangePostResponse}
          options={{ readOnly, minimap: { enabled: false }, lineNumbers: 'on' }}
          className="h-full"
        />
      </TabsContent>

      <TabsContent value="tests" className="flex-1 m-0 p-0">
        <MonacoEditor
          language="javascript"
          value={testsScript}
          onChange={readOnly ? undefined : onChangeTests}
          options={{ readOnly, minimap: { enabled: false }, lineNumbers: 'on' }}
          className="h-full"
        />
      </TabsContent>
    </Tabs>
  )
}
```

> **Note to subagent:** Check the actual import path for `MonacoEditor` in this codebase — it may be `@/components/editor/MonacoWrapper` or similar. Use whatever the existing multi-line Monaco wrapper is called.

- [ ] **Step 2: Add Scripts tab to `RequestEditor.tsx`**

Open `frontend/src/components/request/RequestEditor.tsx`. Find the tab strip (the `TabsList` containing Body, Headers, Params, Auth, etc.) and add:

```tsx
<TabsTrigger value="scripts">Scripts</TabsTrigger>
<TabsTrigger value="vars">Vars</TabsTrigger>
```

Add the corresponding `TabsContent`:

```tsx
<TabsContent value="scripts" className="flex-1 m-0 p-0">
  <ScriptsTab
    preRequestScript={request.preRequestScript ?? ''}
    postResponseScript={request.postResponseScript ?? ''}
    testsScript={request.tests ?? ''}
    onChangePreRequest={(v) => updateRequest({ preRequestScript: v })}
    onChangePostResponse={(v) => updateRequest({ postResponseScript: v })}
    onChangeTests={(v) => updateRequest({ tests: v })}
  />
</TabsContent>
```

> **Note to subagent:** `request.preRequestScript`, `request.postResponseScript`, `request.tests` — check the actual field names on the request type in `frontend/src/types/`. Use the real names. `updateRequest` is whatever the local state setter is called in `RequestEditor`.

- [ ] **Step 3: TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | head -30
```

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/request/ScriptsTab.tsx frontend/src/components/request/RequestEditor.tsx
git commit -m "feat(frontend): ScriptsTab with Monaco sub-tabs for pre/post/tests scripts"
```

---

## Task 2: `VarsTab` component (declarative pre/post variable tables)

**Files:**
- Create: `frontend/src/components/request/VarsTab.tsx`
- Modify: `frontend/src/components/request/RequestEditor.tsx`

- [ ] **Step 1: Create `frontend/src/components/request/VarsTab.tsx`**

```tsx
import { useState } from 'react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Plus, Trash2 } from 'lucide-react'

export interface VarRow {
  name: string
  value: string
  enabled: boolean
}

interface VarsTabProps {
  preRequestVars: VarRow[]
  postResponseVars: VarRow[]
  onChangePreRequest: (vars: VarRow[]) => void
  onChangePostResponse: (vars: VarRow[]) => void
}

function VarTable({
  rows,
  onChange,
  valuePlaceholder,
}: {
  rows: VarRow[]
  onChange: (rows: VarRow[]) => void
  valuePlaceholder: string
}) {
  const add = () => onChange([...rows, { name: '', value: '', enabled: true }])

  const update = (i: number, patch: Partial<VarRow>) =>
    onChange(rows.map((r, idx) => (idx === i ? { ...r, ...patch } : r)))

  const remove = (i: number) => onChange(rows.filter((_, idx) => idx !== i))

  return (
    <div className="flex flex-col gap-1 p-2">
      {rows.map((row, i) => (
        <div key={i} className="flex items-center gap-2">
          <Input
            value={row.name}
            onChange={(e) => update(i, { name: e.target.value })}
            placeholder="name"
            className="h-7 text-xs flex-1"
          />
          <Input
            value={row.value}
            onChange={(e) => update(i, { value: e.target.value })}
            placeholder={valuePlaceholder}
            className="h-7 text-xs flex-1 font-mono"
          />
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7 shrink-0"
            onClick={() => remove(i)}
          >
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        </div>
      ))}
      <Button
        variant="ghost"
        size="sm"
        className="self-start h-7 gap-1 text-xs mt-1"
        onClick={add}
      >
        <Plus className="h-3.5 w-3.5" />
        Add variable
      </Button>
    </div>
  )
}

export function VarsTab({
  preRequestVars,
  postResponseVars,
  onChangePreRequest,
  onChangePostResponse,
}: VarsTabProps) {
  return (
    <Tabs defaultValue="pre-request" className="flex flex-col h-full">
      <TabsList className="shrink-0 w-full justify-start rounded-none border-b bg-transparent px-2">
        <TabsTrigger value="pre-request" className="text-xs">
          Pre Request
        </TabsTrigger>
        <TabsTrigger value="post-response" className="text-xs">
          Post Response
        </TabsTrigger>
      </TabsList>

      <TabsContent value="pre-request" className="flex-1 overflow-auto">
        <div className="p-2 pb-0">
          <p className="text-xs text-muted-foreground mb-2">
            Set variables before the request. Values are JS literals (strings, numbers, booleans).
          </p>
        </div>
        <VarTable
          rows={preRequestVars}
          onChange={onChangePreRequest}
          valuePlaceholder="value (JS literal)"
        />
      </TabsContent>

      <TabsContent value="post-response" className="flex-1 overflow-auto">
        <div className="p-2 pb-0">
          <p className="text-xs text-muted-foreground mb-2">
            Extract values from the response. Values are JS expressions — <code className="text-xs">res</code> is available.
          </p>
        </div>
        <VarTable
          rows={postResponseVars}
          onChange={onChangePostResponse}
          valuePlaceholder="expression (e.g. res.getBody().token)"
        />
      </TabsContent>
    </Tabs>
  )
}
```

- [ ] **Step 2: Add Vars tab to `RequestEditor.tsx`**

Import `VarsTab` and add its `TabsContent` (the trigger was added in Task 1 Step 2):

```tsx
import { VarsTab } from './VarsTab'

// In the TabsContent area:
<TabsContent value="vars" className="flex-1 m-0 p-0">
  <VarsTab
    preRequestVars={request.variables ?? []}
    postResponseVars={request.actions?.map(a => ({
      name: a.variable?.name ?? '',
      value: a.selector?.expression ?? '',
      enabled: !a.disabled,
    })) ?? []}
    onChangePreRequest={(vars) => updateRequest({ variables: vars })}
    onChangePostResponse={(vars) => updateRequest({
      actions: vars.map(v => ({
        type: 'set-variable' as const,
        phase: 'after-response' as const,
        selector: { expression: v.value, method: 'jsonq' },
        variable: { name: v.name, scope: 'runtime' },
        disabled: !v.enabled,
      }))
    })}
  />
</TabsContent>
```

> **Note to subagent:** `request.variables` and `request.actions` — check the actual TypeScript request type. The post-response vars map to `OcAction.SetVariable` entries in the spec. Adjust field names to match the real types.

- [ ] **Step 3: TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | head -30
```

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/request/VarsTab.tsx frontend/src/components/request/RequestEditor.tsx
git commit -m "feat(frontend): VarsTab with pre/post declarative variable tables"
```

---

## Task 3: `TestsPanel` in response area + Console wiring

**Files:**
- Create: `frontend/src/components/response/TestsPanel.tsx`
- Modify: `frontend/src/components/response/ResponsePanel.tsx`
- Modify: `frontend/src/stores/consoleStore.ts`

- [ ] **Step 1: Create `frontend/src/components/response/TestsPanel.tsx`**

```tsx
import { Badge } from '@/components/ui/badge'
import { CheckCircle2, XCircle } from 'lucide-react'
import type { TestResult } from '@/types/scripting'

interface TestsPanelProps {
  results: TestResult[]
}

export function TestsPanel({ results }: TestsPanelProps) {
  if (results.length === 0) {
    return (
      <div className="flex items-center justify-center h-full text-sm text-muted-foreground">
        No tests ran for this request.
      </div>
    )
  }

  const passed = results.filter((r) => r.status === 'passed').length
  const failed = results.filter((r) => r.status === 'failed').length

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center gap-2 px-3 py-2 border-b shrink-0">
        {passed > 0 && (
          <Badge variant="outline" className="gap-1 text-green-600 border-green-200 bg-green-50 dark:bg-green-950 dark:border-green-800 dark:text-green-400">
            <CheckCircle2 className="h-3 w-3" />
            {passed} passed
          </Badge>
        )}
        {failed > 0 && (
          <Badge variant="outline" className="gap-1 text-red-600 border-red-200 bg-red-50 dark:bg-red-950 dark:border-red-800 dark:text-red-400">
            <XCircle className="h-3 w-3" />
            {failed} failed
          </Badge>
        )}
      </div>

      <div className="flex-1 overflow-auto">
        {results.map((result, i) => (
          <div
            key={i}
            className="flex items-start gap-2 px-3 py-2 border-b last:border-b-0 text-sm"
          >
            {result.status === 'passed' ? (
              <CheckCircle2 className="h-4 w-4 text-green-500 mt-0.5 shrink-0" />
            ) : (
              <XCircle className="h-4 w-4 text-red-500 mt-0.5 shrink-0" />
            )}
            <div className="flex flex-col gap-0.5 min-w-0">
              <span className={result.status === 'failed' ? 'text-foreground' : 'text-muted-foreground'}>
                {result.name}
              </span>
              {result.error && (
                <span className="text-xs text-red-500 font-mono break-all">
                  {result.error}
                </span>
              )}
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}
```

- [ ] **Step 2: Add Tests tab to `ResponsePanel.tsx`**

Open `frontend/src/components/response/ResponsePanel.tsx`. Find where `testResults` comes from (it should now be part of the request execution result stored in the response store or local state).

Add to the tab strip:

```tsx
<TabsTrigger value="tests">
  Tests
  {testResults.length > 0 && (
    <Badge
      variant="outline"
      className={`ml-1.5 text-xs px-1 py-0 h-4 ${
        testResults.some(r => r.status === 'failed')
          ? 'border-red-300 text-red-600'
          : 'border-green-300 text-green-600'
      }`}
    >
      {testResults.filter(r => r.status === 'passed').length}/
      {testResults.length}
    </Badge>
  )}
</TabsTrigger>
```

Add `TabsContent`:

```tsx
import { TestsPanel } from './TestsPanel'
import type { TestResult } from '@/types/scripting'

<TabsContent value="tests" className="flex-1 m-0 p-0">
  <TestsPanel results={testResults} />
</TabsContent>
```

> **Note to subagent:** `testResults` should come from the same place response data comes from — likely the response store or local state after `execute_request` returns. Find where `ExecuteRequestResponse` is consumed and destructure `testResults` from it. Pass it down to `ResponsePanel`.

- [ ] **Step 3: Wire console entries to `consoleStore`**

Open `frontend/src/stores/consoleStore.ts`. Find where domain events are handled (likely a Tauri event listener). Add handling for `ConsoleOutput`:

```typescript
import type { ConsoleEntry } from '@/types/scripting'

// In the event listener setup (wherever other domain events are listened to):
await listen<{ requestName: string; entries: ConsoleEntry[] }>(
  'console_output',
  (event) => {
    const { requestName, entries } = event.payload
    entries.forEach((entry) => {
      // Format: [RequestName / phase-info] message
      const prefix = `[${requestName}]`
      useConsoleStore.getState().addEntry({
        level: entry.level,
        message: `${prefix} ${entry.message}`,
        timestamp: new Date().toISOString(),
      })
    })
  }
)
```

> **Note to subagent:** The event name `'console_output'` must match what `TauriEventBus` uses when publishing `DomainEvent::ConsoleOutput`. Check `crates/rocket-infra/src/event_bus.rs` (or wherever events are serialised to Tauri event names) and use the exact string. Also check the actual shape of `addEntry` in `consoleStore.ts` — adjust the call to match.

- [ ] **Step 4: TypeScript check**

```bash
cd frontend && yarn tsc --noEmit 2>&1 | head -30
```

Expected: zero errors.

- [ ] **Step 5: Full compile check (Rust + TypeScript)**

```bash
cargo check -p rocket-tauri 2>&1 | grep "^error" | head -10
cd frontend && yarn tsc --noEmit 2>&1 | head -10
```

Expected: zero errors in both.

- [ ] **Step 6: Update `crates/rocket-scripting/CLAUDE.md`** with any implementation notes that changed during execution (only if something a fresh subagent couldn't infer from code).

- [ ] **Step 7: Commit**

```bash
git add frontend/src/components/response/TestsPanel.tsx \
        frontend/src/components/response/ResponsePanel.tsx \
        frontend/src/stores/consoleStore.ts \
        crates/rocket-scripting/CLAUDE.md
git commit -m "feat(frontend): TestsPanel, Console event wiring, CLAUDE.md update"
```
