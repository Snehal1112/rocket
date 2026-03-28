# Console Tab Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a Postman-style Console panel that slides up above the GlobalStatusBar, showing every request sent during the session with expandable request/response detail.

**Architecture:** A Zustand in-memory store (`useConsoleStore`) holds up to 200 `ConsoleEntry` objects. `App.tsx` feeds entries via the existing `onRequestSent` callback. A resizable `ConsolePanel` component renders between the main content and `GlobalStatusBar`. `GlobalStatusBar` gains a Console toggle button.

**Tech Stack:** React 18, TypeScript, Zustand, Tailwind CSS, Vitest, @testing-library/react

---

### Task 1: Add `ConsoleEntry` type

**Files:**
- Modify: `frontend/src/types/index.ts` (append at end of file)

**Step 1: Add the type**

Append to `frontend/src/types/index.ts`:

```ts
export interface ConsoleEntry {
  id: string
  timestamp: string
  method: HttpMethod
  url: string
  status: number
  statusText: string
  duration: number
  size: number
  requestHeaders: Record<string, string>
  requestBody: string
  responseHeaders: Record<string, string>
  responseBody: string
}
```

**Step 2: Verify TypeScript compiles**

```bash
cd /home/numericlabs/data/rocket-api/frontend && yarn tsc --noEmit
```

Expected: no errors.

**Step 3: Commit**

```bash
git add frontend/src/types/index.ts
git commit -m "feat(types): add ConsoleEntry interface"
```

---

### Task 2: Create `useConsoleStore`

**Files:**
- Create: `frontend/src/store/console.ts`
- Test: `frontend/src/store/__tests__/console.test.ts`

**Step 1: Write the failing tests**

Create `frontend/src/store/__tests__/console.test.ts`:

```ts
import { describe, beforeEach, it, expect } from 'vitest'
import { useConsoleStore } from '@/store/console'
import { HttpRequest, HttpResponse } from '@/types'

const mockReq: HttpRequest = {
  id: 'req-1',
  name: 'Test',
  method: 'GET',
  url: 'https://api.example.com/users',
  headers: [{ key: 'Accept', value: 'application/json', enabled: true }],
  body: { type: 'none', content: '' },
  queryParams: [],
  auth: { type: 'none' },
}

const mockRes: HttpResponse = {
  status: 200,
  statusText: 'OK',
  headers: { 'content-type': 'application/json' },
  body: '{"users":[]}',
  size: 12,
  time: 142,
}

describe('useConsoleStore', () => {
  beforeEach(() => {
    useConsoleStore.setState({ entries: [] })
  })

  it('adds an entry with mapped fields', () => {
    useConsoleStore.getState().addEntry(mockReq, mockRes)
    const entries = useConsoleStore.getState().entries
    expect(entries).toHaveLength(1)
    expect(entries[0].method).toBe('GET')
    expect(entries[0].url).toBe('https://api.example.com/users')
    expect(entries[0].status).toBe(200)
    expect(entries[0].duration).toBe(142)
    expect(entries[0].requestHeaders).toEqual({ Accept: 'application/json' })
    expect(entries[0].requestBody).toBe('')
    expect(entries[0].responseBody).toBe('{"users":[]}')
  })

  it('prepends new entries (newest first)', () => {
    useConsoleStore.getState().addEntry(mockReq, mockRes)
    useConsoleStore.getState().addEntry({ ...mockReq, url: 'https://api.example.com/posts' }, mockRes)
    const entries = useConsoleStore.getState().entries
    expect(entries[0].url).toBe('https://api.example.com/posts')
  })

  it('excludes disabled request headers', () => {
    const reqWithDisabled: HttpRequest = {
      ...mockReq,
      headers: [
        { key: 'Accept', value: 'application/json', enabled: true },
        { key: 'X-Debug', value: '1', enabled: false },
      ],
    }
    useConsoleStore.getState().addEntry(reqWithDisabled, mockRes)
    expect(useConsoleStore.getState().entries[0].requestHeaders).toEqual({
      Accept: 'application/json',
    })
  })

  it('clears all entries', () => {
    useConsoleStore.getState().addEntry(mockReq, mockRes)
    useConsoleStore.getState().clearEntries()
    expect(useConsoleStore.getState().entries).toHaveLength(0)
  })

  it('caps entries at 200, dropping oldest', () => {
    for (let i = 0; i < 201; i++) {
      useConsoleStore.getState().addEntry({ ...mockReq, url: `https://api.example.com/${i}` }, mockRes)
    }
    const entries = useConsoleStore.getState().entries
    expect(entries).toHaveLength(200)
    // Newest is first (i=200), oldest dropped was i=0
    expect(entries[0].url).toBe('https://api.example.com/200')
    expect(entries.find(e => e.url === 'https://api.example.com/0')).toBeUndefined()
  })
})
```

**Step 2: Run tests — verify they fail**

```bash
cd /home/numericlabs/data/rocket-api/frontend && yarn test src/store/__tests__/console.test.ts --reporter=verbose
```

Expected: FAIL with "Cannot find module '@/store/console'".

**Step 3: Implement the store**

Create `frontend/src/store/console.ts`:

```ts
import { create } from 'zustand'
import { ConsoleEntry, HttpRequest, HttpResponse } from '@/types'

const MAX_ENTRIES = 200

interface ConsoleState {
  entries: ConsoleEntry[]
  addEntry: (req: HttpRequest, res: HttpResponse) => void
  clearEntries: () => void
}

export const useConsoleStore = create<ConsoleState>((set) => ({
  entries: [],

  addEntry: (req, res) => {
    const entry: ConsoleEntry = {
      id: crypto.randomUUID(),
      timestamp: new Date().toISOString(),
      method: req.method,
      url: req.url,
      status: res.status,
      statusText: res.statusText,
      duration: res.time,
      size: res.size,
      requestHeaders: req.headers
        .filter(h => h.enabled)
        .reduce<Record<string, string>>((acc, h) => ({ ...acc, [h.key]: h.value }), {}),
      requestBody: req.body.content,
      responseHeaders: res.headers,
      responseBody: res.body,
    }
    set(state => ({
      entries: [entry, ...state.entries].slice(0, MAX_ENTRIES),
    }))
  },

  clearEntries: () => set({ entries: [] }),
}))
```

**Step 4: Run tests — verify they pass**

```bash
cd /home/numericlabs/data/rocket-api/frontend && yarn test src/store/__tests__/console.test.ts --reporter=verbose
```

Expected: 5 tests PASS.

**Step 5: Commit**

```bash
git add frontend/src/store/console.ts frontend/src/store/__tests__/console.test.ts
git commit -m "feat(store): add useConsoleStore with 200-entry cap"
```

---

### Task 3: Create `ConsolePanel` component

**Files:**
- Create: `frontend/src/components/layout/ConsolePanel.tsx`
- Test: `frontend/src/components/layout/__tests__/ConsolePanel.test.tsx`

**Step 1: Write the failing tests**

Create `frontend/src/components/layout/__tests__/ConsolePanel.test.tsx`:

```tsx
import { render, screen, fireEvent } from '@testing-library/react'
import { describe, beforeEach, it, expect, vi } from 'vitest'
import { ConsolePanel } from '@/components/layout/ConsolePanel'
import { useConsoleStore } from '@/store/console'

const defaultProps = {
  isOpen: true,
  height: 280,
  onHeightChange: vi.fn(),
}

describe('ConsolePanel', () => {
  beforeEach(() => {
    useConsoleStore.setState({ entries: [] })
  })

  it('renders nothing when closed', () => {
    const { container } = render(<ConsolePanel {...defaultProps} isOpen={false} />)
    expect(container.firstChild).toBeNull()
  })

  it('shows empty state when no entries', () => {
    render(<ConsolePanel {...defaultProps} />)
    expect(screen.getByText('No requests sent yet')).toBeInTheDocument()
  })

  it('shows an entry row with method, url, status', () => {
    useConsoleStore.setState({
      entries: [{
        id: '1',
        timestamp: new Date().toISOString(),
        method: 'GET',
        url: 'https://api.example.com/users',
        status: 200,
        statusText: 'OK',
        duration: 142,
        size: 12,
        requestHeaders: {},
        requestBody: '',
        responseHeaders: {},
        responseBody: '{}',
      }],
    })
    render(<ConsolePanel {...defaultProps} />)
    expect(screen.getByText('GET')).toBeInTheDocument()
    expect(screen.getByText(/api\.example\.com\/users/)).toBeInTheDocument()
    expect(screen.getByText('200')).toBeInTheDocument()
  })

  it('expands entry detail on row click', () => {
    useConsoleStore.setState({
      entries: [{
        id: '1',
        timestamp: new Date().toISOString(),
        method: 'POST',
        url: 'https://api.example.com/users',
        status: 201,
        statusText: 'Created',
        duration: 88,
        size: 24,
        requestHeaders: { 'Content-Type': 'application/json' },
        requestBody: '{"name":"Alice"}',
        responseHeaders: { 'content-type': 'application/json' },
        responseBody: '{"id":1}',
      }],
    })
    render(<ConsolePanel {...defaultProps} />)
    // Detail not visible initially
    expect(screen.queryByText('Request Headers')).not.toBeInTheDocument()
    // Click the row
    fireEvent.click(screen.getByText('POST').closest('[data-testid="console-entry-row"]')!)
    // Detail now visible
    expect(screen.getByText('Request Headers')).toBeInTheDocument()
    expect(screen.getByText('Content-Type')).toBeInTheDocument()
    expect(screen.getByText('Response Body')).toBeInTheDocument()
  })

  it('clears entries when Clear button is clicked', () => {
    useConsoleStore.setState({
      entries: [{
        id: '1',
        timestamp: new Date().toISOString(),
        method: 'GET',
        url: 'https://api.example.com',
        status: 200,
        statusText: 'OK',
        duration: 50,
        size: 0,
        requestHeaders: {},
        requestBody: '',
        responseHeaders: {},
        responseBody: '',
      }],
    })
    render(<ConsolePanel {...defaultProps} />)
    fireEvent.click(screen.getByRole('button', { name: /clear/i }))
    expect(screen.getByText('No requests sent yet')).toBeInTheDocument()
  })
})
```

**Step 2: Run tests — verify they fail**

```bash
cd /home/numericlabs/data/rocket-api/frontend && yarn test src/components/layout/__tests__/ConsolePanel.test.tsx --reporter=verbose
```

Expected: FAIL with "Cannot find module '@/components/layout/ConsolePanel'".

**Step 3: Implement ConsolePanel**

Create `frontend/src/components/layout/ConsolePanel.tsx`:

```tsx
import { useRef, useState } from 'react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { useConsoleStore } from '@/store/console'
import { ConsoleEntry } from '@/types'
import { ChevronDown, ChevronRight } from 'lucide-react'

interface ConsolePanelProps {
  isOpen: boolean
  height: number
  onHeightChange: (height: number) => void
}

const MIN_HEIGHT = 120
const MAX_HEIGHT = 600

function statusColor(status: number): string {
  if (status >= 500) return 'text-red-500'
  if (status >= 400) return 'text-orange-500'
  if (status >= 300) return 'text-yellow-500'
  if (status >= 200) return 'text-green-500'
  return 'text-muted-foreground'
}

function formatTime(iso: string): string {
  const d = new Date(iso)
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false })
}

function EntryDetail({ entry }: { entry: ConsoleEntry }) {
  const sections: Array<{ label: string; content: string }> = [
    { label: 'Request Headers', content: Object.entries(entry.requestHeaders).map(([k, v]) => `${k}: ${v}`).join('\n') || '(none)' },
    { label: 'Request Body', content: entry.requestBody || '(empty)' },
    { label: 'Response Headers', content: Object.entries(entry.responseHeaders).map(([k, v]) => `${k}: ${v}`).join('\n') || '(none)' },
    { label: 'Response Body', content: entry.responseBody || '(empty)' },
  ]

  return (
    <div className="grid grid-cols-2 gap-2 px-4 py-2 bg-muted/30 border-t text-[11px]">
      {sections.map(s => (
        <div key={s.label}>
          <div className="font-medium text-muted-foreground mb-1">{s.label}</div>
          <pre className="font-mono whitespace-pre-wrap break-all bg-background/60 rounded p-1.5 max-h-32 overflow-auto">
            {s.content}
          </pre>
        </div>
      ))}
    </div>
  )
}

export function ConsolePanel({ isOpen, height, onHeightChange }: ConsolePanelProps) {
  const { entries, clearEntries } = useConsoleStore()
  const [search, setSearch] = useState('')
  const [expandedId, setExpandedId] = useState<string | null>(null)
  const dragStartRef = useRef<{ y: number; h: number } | null>(null)

  if (!isOpen) return null

  const filtered = search
    ? entries.filter(e => e.url.toLowerCase().includes(search.toLowerCase()))
    : entries

  const handleDragMouseDown = (e: React.MouseEvent) => {
    e.preventDefault()
    dragStartRef.current = { y: e.clientY, h: height }

    const onMouseMove = (ev: MouseEvent) => {
      if (!dragStartRef.current) return
      const delta = dragStartRef.current.y - ev.clientY
      const next = Math.max(MIN_HEIGHT, Math.min(MAX_HEIGHT, dragStartRef.current.h + delta))
      onHeightChange(next)
    }

    const onMouseUp = () => {
      dragStartRef.current = null
      document.removeEventListener('mousemove', onMouseMove)
      document.removeEventListener('mouseup', onMouseUp)
    }

    document.addEventListener('mousemove', onMouseMove)
    document.addEventListener('mouseup', onMouseUp)
  }

  return (
    <div
      className="shrink-0 border-t border-border/70 bg-card/85 flex flex-col"
      style={{ height }}
    >
      {/* Drag handle. */}
      <div
        className="h-1 cursor-row-resize bg-border/40 hover:bg-primary/40 transition-colors shrink-0"
        onMouseDown={handleDragMouseDown}
      />

      {/* Tab strip + toolbar. */}
      <div className="flex items-center gap-2 px-3 py-1.5 border-b border-border/70 shrink-0">
        <span className="text-xs font-medium">Console</span>
        {entries.length > 0 && (
          <span className="text-[10px] px-1.5 py-0.5 rounded-full bg-muted text-muted-foreground">
            {entries.length}
          </span>
        )}
        <div className="flex-1" />
        <Input
          placeholder="Filter by URL"
          value={search}
          onChange={e => setSearch(e.target.value)}
          className="h-6 text-xs w-48"
        />
        <Button
          variant="ghost"
          size="sm"
          className="h-6 px-2 text-xs"
          onClick={clearEntries}
        >
          Clear
        </Button>
      </div>

      {/* Entry list. */}
      <div className="flex-1 overflow-y-auto text-[11px] font-mono">
        {filtered.length === 0 ? (
          <div className="flex items-center justify-center h-full text-muted-foreground text-xs">
            No requests sent yet
          </div>
        ) : (
          filtered.map(entry => (
            <div key={entry.id}>
              <div
                data-testid="console-entry-row"
                className="flex items-center gap-2 px-3 py-1 hover:bg-accent/40 cursor-pointer border-b border-border/30"
                onClick={() => setExpandedId(expandedId === entry.id ? null : entry.id)}
              >
                {expandedId === entry.id
                  ? <ChevronDown className="h-3 w-3 shrink-0 text-muted-foreground" />
                  : <ChevronRight className="h-3 w-3 shrink-0 text-muted-foreground" />
                }
                <span className="text-muted-foreground w-16 shrink-0">{formatTime(entry.timestamp)}</span>
                <span className="font-semibold w-12 shrink-0">{entry.method}</span>
                <span className="flex-1 truncate text-foreground/80">{entry.url}</span>
                <span className={`w-10 text-right shrink-0 font-semibold ${statusColor(entry.status)}`}>
                  {entry.status}
                </span>
                <span className="text-muted-foreground w-16 text-right shrink-0">
                  {entry.duration}ms
                </span>
              </div>
              {expandedId === entry.id && <EntryDetail entry={entry} />}
            </div>
          ))
        )}
      </div>
    </div>
  )
}
```

**Step 4: Run tests — verify they pass**

```bash
cd /home/numericlabs/data/rocket-api/frontend && yarn test src/components/layout/__tests__/ConsolePanel.test.tsx --reporter=verbose
```

Expected: 5 tests PASS.

**Step 5: Commit**

```bash
git add frontend/src/components/layout/ConsolePanel.tsx frontend/src/components/layout/__tests__/ConsolePanel.test.tsx
git commit -m "feat(frontend): add ConsolePanel component"
```

---

### Task 4: Add Console toggle to `GlobalStatusBar`

**Files:**
- Modify: `frontend/src/components/layout/GlobalStatusBar.tsx`
- Test: `frontend/src/components/layout/__tests__/GlobalStatusBar.test.tsx`

**Step 1: Write the failing test**

Create `frontend/src/components/layout/__tests__/GlobalStatusBar.test.tsx`:

```tsx
import { render, screen, fireEvent } from '@testing-library/react'
import { describe, it, expect, vi } from 'vitest'
import { GlobalStatusBar } from '@/components/layout/GlobalStatusBar'

// Mock the API calls that GlobalStatusBar triggers internally.
vi.mock('@/lib/api', () => ({
  apiService: {
    getTemplates: vi.fn().mockResolvedValue([]),
    getTemplateCategories: vi.fn().mockResolvedValue([]),
    getCookies: vi.fn().mockResolvedValue([]),
    getCookieDomains: vi.fn().mockResolvedValue([]),
  },
}))

vi.mock('@/store/collections', () => ({
  useCollectionsStore: () => ({
    createCollection: vi.fn(),
    importBruno: vi.fn(),
  }),
}))

vi.mock('@/store/tabs-store', () => ({
  useTabsStore: () => ({ loadRequestInActiveTab: vi.fn() }),
}))

describe('GlobalStatusBar', () => {
  it('renders a Console toggle button', () => {
    render(<GlobalStatusBar isConsoleOpen={false} onConsoleToggle={vi.fn()} />)
    expect(screen.getByRole('button', { name: /console/i })).toBeInTheDocument()
  })

  it('calls onConsoleToggle when Console button is clicked', () => {
    const toggle = vi.fn()
    render(<GlobalStatusBar isConsoleOpen={false} onConsoleToggle={toggle} />)
    fireEvent.click(screen.getByRole('button', { name: /console/i }))
    expect(toggle).toHaveBeenCalledTimes(1)
  })

  it('shows active state when console is open', () => {
    render(<GlobalStatusBar isConsoleOpen={true} onConsoleToggle={vi.fn()} />)
    const btn = screen.getByRole('button', { name: /console/i })
    expect(btn.className).toMatch(/bg-accent/)
  })
})
```

**Step 2: Run test — verify it fails**

```bash
cd /home/numericlabs/data/rocket-api/frontend && yarn test src/components/layout/__tests__/GlobalStatusBar.test.tsx --reporter=verbose
```

Expected: FAIL — GlobalStatusBar does not accept those props yet.

**Step 3: Update GlobalStatusBar**

In `frontend/src/components/layout/GlobalStatusBar.tsx`, add props interface and Console button:

At the top of the file, after the imports, add the interface. Change:
```tsx
export function GlobalStatusBar() {
```
To:
```tsx
interface GlobalStatusBarProps {
  isConsoleOpen: boolean
  onConsoleToggle: () => void
}

export function GlobalStatusBar({ isConsoleOpen, onConsoleToggle }: GlobalStatusBarProps) {
```

Then in the JSX, add the Console button on the **right side** of the status bar strip. The current strip div ends with the Cookies button. Add a spacer and the Console button after it:

```tsx
      {/* existing buttons: New, Import, Templates, Cookies */}
      <div className="flex-1" />
      <Button
        variant="ghost"
        size="sm"
        className={`h-5 px-1.5 text-[11px] gap-1 ${isConsoleOpen ? 'bg-accent' : ''}`}
        onClick={onConsoleToggle}
        title="Toggle Console"
        aria-label="Console"
      >
        Console
      </Button>
```

**Step 4: Run test — verify it passes**

```bash
cd /home/numericlabs/data/rocket-api/frontend && yarn test src/components/layout/__tests__/GlobalStatusBar.test.tsx --reporter=verbose
```

Expected: 3 tests PASS.

**Step 5: Commit**

```bash
git add frontend/src/components/layout/GlobalStatusBar.tsx frontend/src/components/layout/__tests__/GlobalStatusBar.test.tsx
git commit -m "feat(frontend): add Console toggle to GlobalStatusBar"
```

---

### Task 5: Wire everything in `App.tsx`

**Files:**
- Modify: `frontend/src/App.tsx`

**Step 1: Update App.tsx**

Add these imports at the top:
```tsx
import { ConsolePanel } from '@/components/layout/ConsolePanel'
import { useConsoleStore } from '@/store/console'
import { useState } from 'react'  // already imported — just add if missing
```

Inside the `App` function, add state:
```tsx
const [isConsoleOpen, setIsConsoleOpen] = useState(false)
const [consoleHeight, setConsoleHeight] = useState(280)
```

Update `onRequestSent` callback (currently just `console.log`):
```tsx
onRequestSent={(req, res) => {
  useConsoleStore.getState().addEntry(req, res)
  if (!isConsoleOpen) setIsConsoleOpen(true)
}}
```

Add `<ConsolePanel>` between the `</div>` that closes the main flex row and `<GlobalStatusBar>`:
```tsx
          </div>  {/* closes the flex-1 row */}

          <ConsolePanel
            isOpen={isConsoleOpen}
            height={consoleHeight}
            onHeightChange={setConsoleHeight}
          />

          <GlobalStatusBar
            isConsoleOpen={isConsoleOpen}
            onConsoleToggle={() => setIsConsoleOpen(o => !o)}
          />
```

**Step 2: Verify TypeScript compiles**

```bash
cd /home/numericlabs/data/rocket-api/frontend && yarn tsc --noEmit
```

Expected: no errors.

**Step 3: Run all tests**

```bash
cd /home/numericlabs/data/rocket-api/frontend && yarn test --reporter=verbose
```

Expected: all tests PASS (store, ConsolePanel, GlobalStatusBar, RequestBuilderTabs).

**Step 4: Commit**

```bash
git add frontend/src/App.tsx
git commit -m "feat(frontend): wire ConsolePanel into App layout"
```
