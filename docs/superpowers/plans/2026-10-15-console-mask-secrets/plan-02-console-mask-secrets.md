# Console secret masking: panel, reveal toggle and docs Implementation Plan

> **For agentic workers:** This plan is executed by a `factory-builder` subagent through `superpowers:executing-plans`, skipping that skill's final whole-branch review, ledger scripts and progress file. Steps use checkbox (`- [ ]`) syntax for tracking. Do not run any commit step: the driver commits.

**Goal:** Make the Console panel show the masked copy of each HTTP row, add a per-row reveal toggle, make search match the displayed text, and document the rule.

**Architecture:** Plan 1 put `masked` and `uncertain` on `HttpConsoleEntry` and exported `selectHttpView` and `hasHiddenValues` from `src/lib/console-redaction.ts`. `ConsolePanel` keeps one `revealedId` state next to `expandedId`, passes the selected view to the row and the detail, and resets the reveal whenever the expanded row changes.

**Tech Stack:** React 19, TypeScript, shadcn `Button`, lucide-react icons, Vitest with Testing Library, Biome.

**Spec:** `docs/superpowers/specs/2026-10-15-console-mask-secrets-design.md`

## Global Constraints

- UI uses shadcn primitives only: the toggle is the existing `Button` from `@/components/ui/button`. Icons come from `lucide-react` only (`Eye`, `EyeOff`).
- The reveal is local to the open row: collapsing the row, opening another row or closing the panel resets it. It is never stored.
- An entry without `masked` (a Flow row) renders raw exactly as today and shows no toggle.
- A row with nothing hidden shows no toggle. An uncertain row shows the note `May contain secrets` and no toggle, and is never revealed.
- Search matches the displayed text: the masked URL, or the raw URL only for the revealed row.
- Copy actions: the Console has none today, so nothing is added. Any future copy action must copy what `selectHttpView` returns.
- Only the single-request path changes. No Rust change. Use `yarn` only. No unwrap-style shortcuts.

## Review Focus

- A masked secret must not be findable by searching for it, and a revealed row must be findable by its raw URL. Pinned by `does not find a masked row by its secret` and `finds the revealed row by its raw url` in Task 1.
- Collapsing and reopening a revealed row must mask it again. Pinned by `masks the row again after it is collapsed and reopened` in Task 1.
- The collapsed row line must show the masked URL, not only the detail. Pinned by `shows the masked url in the row line` in Task 1.
- A Flow row without `masked` must render raw with no toggle. Pinned by `renders a row without a masked copy as before` in Task 1.
- An uncertain row must show the note, no toggle, and the masked text. Pinned by `shows the note and no toggle for an uncertain row` in Task 1.

---

### Task 1: Panel display, reveal toggle and search

**Files:**
- Modify: `src/components/layout/ConsolePanel.tsx` (imports, `HttpEntryDetail` at lines 37 to 63, `matchesSearch` at lines 163 to 177, the panel body at lines 179 to 335)
- Test: `src/components/layout/__tests__/ConsolePanel.masking.test.tsx` (create)

**Interfaces:**
- Consumes: from plan 1, `selectHttpView(entry, revealed)`, `hasHiddenValues(entry)` and the entry fields `masked`, `uncertain`.
- Produces: the toggle button with accessible names `Show secrets` and `Hide secrets`, and the text `May contain secrets`.

- [ ] **Step 1: Write the failing tests**

Create `src/components/layout/__tests__/ConsolePanel.masking.test.tsx`:

```tsx
import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';
import { useConsoleStore } from '@/stores/console-store';
import { ConsolePanel } from '../ConsolePanel';

const M = '••••••';
const empty = {
  requestHeaders: [],
  requestBody: '',
  responseHeaders: [],
  responseBody: '',
};

function seed(extra: Record<string, unknown> = {}) {
  useConsoleStore.getState().addHttpEntry({
    method: 'GET',
    url: 'https://x.test/a?k=abcdef123',
    status: 200,
    statusText: 'OK',
    durationMs: 3,
    sizeBytes: 1,
    ...empty,
    requestBody: 'token=abcdef123',
    masked: {
      url: `https://x.test/a?k=${M}`,
      ...empty,
      requestBody: `token=${M}`,
    },
    ...extra,
  });
}

function open() {
  render(<ConsolePanel isOpen height={200} onHeightChange={() => undefined} />);
}

function expandRow() {
  fireEvent.click(screen.getByText(`https://x.test/a?k=${M}`));
}

describe('ConsolePanel masking', () => {
  beforeEach(() => useConsoleStore.getState().clearEntries());

  it('shows the masked url in the row line', () => {
    seed();
    open();
    expect(screen.getByText(`https://x.test/a?k=${M}`)).toBeInTheDocument();
    expect(screen.queryByText('https://x.test/a?k=abcdef123')).toBeNull();
  });

  it('shows masked text in the detail by default', () => {
    seed();
    open();
    expandRow();
    expect(screen.getByText(`token=${M}`)).toBeInTheDocument();
    expect(screen.queryByText('token=abcdef123')).toBeNull();
  });

  it('reveals the raw text with the toggle and masks it again', () => {
    seed();
    open();
    expandRow();
    fireEvent.click(screen.getByRole('button', { name: 'Show secrets' }));
    expect(screen.getByText('token=abcdef123')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Hide secrets' }));
    expect(screen.getByText(`token=${M}`)).toBeInTheDocument();
  });

  it('masks the row again after it is collapsed and reopened', () => {
    seed();
    open();
    expandRow();
    fireEvent.click(screen.getByRole('button', { name: 'Show secrets' }));
    fireEvent.click(screen.getByText('https://x.test/a?k=abcdef123'));
    fireEvent.click(screen.getByText(`https://x.test/a?k=${M}`));
    expect(screen.getByText(`token=${M}`)).toBeInTheDocument();
  });

  it('does not find a masked row by its secret', () => {
    seed();
    open();
    fireEvent.change(screen.getByPlaceholderText('Filter by URL or message'), {
      target: { value: 'abcdef123' },
    });
    expect(screen.queryByText(`https://x.test/a?k=${M}`)).toBeNull();
  });

  it('finds the revealed row by its raw url', () => {
    seed();
    open();
    expandRow();
    fireEvent.click(screen.getByRole('button', { name: 'Show secrets' }));
    fireEvent.change(screen.getByPlaceholderText('Filter by URL or message'), {
      target: { value: 'abcdef123' },
    });
    expect(screen.getByText('https://x.test/a?k=abcdef123')).toBeInTheDocument();
  });

  it('renders a row without a masked copy as before', () => {
    useConsoleStore.getState().addHttpEntry({
      method: 'GET',
      url: 'https://flow.test/raw',
      status: 200,
      statusText: 'OK',
      durationMs: 1,
      sizeBytes: 1,
      ...empty,
      requestBody: 'plain-body',
    });
    open();
    fireEvent.click(screen.getByText('https://flow.test/raw'));
    expect(screen.getByText('plain-body')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Show secrets' })).toBeNull();
  });

  it('shows no toggle when nothing is hidden', () => {
    seed({
      url: 'https://x.test/plain',
      requestBody: 'plain',
      masked: { url: 'https://x.test/plain', ...empty, requestBody: 'plain' },
    });
    open();
    fireEvent.click(screen.getByText('https://x.test/plain'));
    expect(screen.queryByRole('button', { name: 'Show secrets' })).toBeNull();
  });

  it('shows the note and no toggle for an uncertain row', () => {
    seed({ uncertain: true });
    open();
    expandRow();
    expect(screen.getByText('May contain secrets')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Show secrets' })).toBeNull();
    expect(screen.getByText(`token=${M}`)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the tests and see them fail**

Run: `yarn test ConsolePanel.masking`
Expected: FAIL. The row still shows the raw URL and there is no toggle.

- [ ] **Step 3: Update the imports of `ConsolePanel.tsx`**

Replace the first import line with:

```tsx
import {
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Eye,
  EyeOff,
  Trash2,
  XCircle,
} from 'lucide-react';
```

and add, among the `@/lib` imports:

```tsx
import { hasHiddenValues, selectHttpView } from '@/lib/console-redaction';
```

- [ ] **Step 4: Replace `HttpEntryDetail`**

Replace the whole `HttpEntryDetail` function with:

```tsx
interface HttpEntryDetailProps {
  entry: HttpConsoleEntry;
  revealed: boolean;
  onToggleReveal: () => void;
}

function HttpEntryDetail({ entry, revealed, onToggleReveal }: HttpEntryDetailProps) {
  const view = selectHttpView(entry, revealed);
  const canReveal = !entry.uncertain && hasHiddenValues(entry);
  const sections = [
    {
      label: 'Request Headers',
      content: view.requestHeaders.map((h) => `${h.key}: ${h.value}`).join('\n') || '(none)',
    },
    { label: 'Request Body', content: view.requestBody || '(empty)' },
    {
      label: 'Response Headers',
      content: view.responseHeaders.map((h) => `${h.key}: ${h.value}`).join('\n') || '(none)',
    },
    { label: 'Response Body', content: view.responseBody || '(empty)' },
  ];

  return (
    <div className='bg-muted/30 border-t text-xs'>
      {(canReveal || entry.uncertain) && (
        <div className='flex items-center justify-end gap-2 px-4 pt-2'>
          {entry.uncertain && (
            <span className='text-muted-foreground'>May contain secrets</span>
          )}
          {canReveal && (
            <Button
              type='button'
              variant='ghost'
              size='sm'
              className='h-6 gap-1 px-2 text-xs'
              aria-label={revealed ? 'Hide secrets' : 'Show secrets'}
              onClick={onToggleReveal}
            >
              {revealed ? <EyeOff className='h-3.5 w-3.5' /> : <Eye className='h-3.5 w-3.5' />}
              {revealed ? 'Hide secrets' : 'Show secrets'}
            </Button>
          )}
        </div>
      )}
      <div className='grid grid-cols-2 gap-2 px-4 py-2'>
        {sections.map((s) => (
          <div key={s.label}>
            <div className='font-medium text-muted-foreground mb-1'>{s.label}</div>
            <pre className='font-mono whitespace-pre-wrap break-all bg-background/60 rounded p-1.5 max-h-32 overflow-auto text-2xs'>
              {s.content}
            </pre>
          </div>
        ))}
      </div>
    </div>
  );
}
```

- [ ] **Step 5: Make `matchesSearch` use the displayed URL**

Change the signature and the http branch:

```tsx
function matchesSearch(entry: PanelEntry, term: string, revealedId: string | null): boolean {
```

and replace `if (entry.kind === 'http') return entry.url.toLowerCase().includes(lower);` with:

```tsx
  if (entry.kind === 'http')
    return selectHttpView(entry, entry.id === revealedId).url.toLowerCase().includes(lower);
```

- [ ] **Step 6: Add the reveal state and wire the rows**

In `ConsolePanel`, after `const [expandedId, setExpandedId] = ...`, add:

```tsx
  const [revealedId, setRevealedId] = useState<string | null>(null);
  // Opening, closing or switching rows always masks again.
  const toggleExpanded = (id: string) => {
    setRevealedId(null);
    setExpandedId((current) => (current === id ? null : id));
  };
```

Change the filter line to `visible.filter((e) => matchesSearch(e, search, revealedId))`.

In the HTTP row, replace both `setExpandedId(expandedId === entry.id ? null : entry.id)` calls (the `onClick` and the `onKeyDown` handler) with `toggleExpanded(entry.id)`. Replace `{entry.url}` in the `flex-1 truncate` span with `{selectHttpView(entry, entry.id === revealedId).url}`. Replace the detail line with:

```tsx
                {expandedId === entry.id && (
                  <HttpEntryDetail
                    entry={entry}
                    revealed={revealedId === entry.id}
                    onToggleReveal={() =>
                      setRevealedId((current) => (current === entry.id ? null : entry.id))
                    }
                  />
                )}
```

- [ ] **Step 7: Run the tests and see them pass**

Run: `yarn test ConsolePanel`
Expected: PASS, including the existing `ConsolePanel.requestName.test.tsx`.

- [ ] **Step 8: Check types and lint**

Run: `yarn tsc --noEmit` then `yarn check`
Expected: no errors. If Biome only reports formatting, run `yarn format` once and re-run `yarn check`.

---

### Task 2: Documentation and final verification

**Files:**
- Create: `docs/features/console-masking.md`
- Modify: `CLAUDE.md` (one reference line in the list that starts with `See .claude/frontend.md`)

**Interfaces:**
- Consumes: the behavior built in plan 1 and Task 1 of this plan.
- Produces: the written rule, so a later change does not unmask a row.

- [ ] **Step 1: Write the rule document**

Create `docs/features/console-masking.md`:

```markdown
# Console secret masking

Issue #43. Spec: `docs/superpowers/specs/2026-10-15-console-mask-secrets-design.md`.

## What is masked

Console HTTP rows written by `sendRequest` carry a masked copy (`HttpConsoleEntry.masked`), fixed at send time by `maskHttpExchange` in `src/lib/console-redaction.ts`. The marker is `••••••`.

- Secret values of the active environment, the global environment and the collection, folder and request variables, in the URL, headers and bodies of the request and the response. Values shorter than 6 characters stay visible, like history and the script engine.
- The values of `Authorization`, `Proxy-Authorization`, `Cookie`, `Set-Cookie` and `X-Api-Key`. The header name stays visible.
- Credentials in the URL (`user:pass@host`) and the value of an API key sent as a query parameter.
- The credentials of the auth block (bearer token, basic and similar passwords, API key value).

## Reveal

The Console detail has a per-row `Show secrets` toggle. It shows the raw fields, which stay in frontend memory only and are never written to disk. Collapsing the row, opening another row or closing the panel masks it again. Search matches the displayed text.

## Rules that must hold

- A value from a secret manager is never resolved, shown or stored raw by the frontend. The helper has no input for it, and a `{{provider.name}}` template passes through unchanged.
- Masking fails closed. When an environment is missing from the query cache, `uncertain` is set: the structural masks (sensitive headers, URL credentials, API key parameter) still apply, the row shows `May contain secrets`, and it cannot be revealed.
- Any copy action added to the Console must copy `selectHttpView(entry, revealed)`, never the raw fields.

## Out of scope

Flow rows (`FlowPane.tsx`) and the Runner still write unmasked rows. The Rust redaction (`redact_secrets`, the script engine) and history are unchanged (#46).
```

- [ ] **Step 2: Reference it from `CLAUDE.md`**

In `CLAUDE.md`, add this line after the line that starts with `See docs/features/collection-trust.md`:

```
See `docs/features/console-masking.md` for the Console secret masking rule (masked copy, reveal toggle, fail-closed, vault values).
```

- [ ] **Step 3: Run the whole frontend verification**

Run: `yarn tsc --noEmit`, `yarn check`, `yarn test console execute-request ConsolePanel`
Expected: all pass.

- [ ] **Step 4: Confirm the scope**

Run: `git diff --stat main`
Expected: only the files named in both plans, `docs/features/console-masking.md` and `CLAUDE.md`. `FlowPane.tsx` and every Rust file must not appear.

- [ ] **Step 5: List the manual check for the merge note**

Do not perform it. Write these three lines at the end of `docs/features/console-masking.md` under a heading `## Manual check`, so the reviewer can run them in the app:

```markdown
## Manual check

1. Send a request that uses a secret environment variable and a Bearer token. The Console row shows `••••••` for both, and the `Authorization` header name stays visible.
2. Open the row and press `Show secrets`. The raw values appear. Collapse and reopen the row: it is masked again.
3. Send a request whose environment is not loaded. The row shows `May contain secrets` and has no toggle.
```

Next plan: none
