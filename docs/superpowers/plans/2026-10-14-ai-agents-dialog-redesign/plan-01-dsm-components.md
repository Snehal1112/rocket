# AI Agents Dialog Redesign: Plan 1, DSM Components Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the seven new `src/components/ui/` components (FormField, SegmentedControl, CollapsibleSection, StateBlock, SecretField, KeyValueRows, DialogBody), each with a Vitest test and a Rocket DSM page entry.

**Architecture:** Each component is a thin wrapper over existing `ui/` primitives (Label, Input, Button, Tooltip, Collapsible, the Radix radio group, SavedPill, Dialog). They are controlled, stateless where possible, and carry no domain logic. Plans 2 and 3 compose them into the AI Agents dialog. The DSM page (`docs/dsm/index.html`) gains one section per component, with a rendered specimen and a spec table in the page's existing `c-` class markup.

**Tech Stack:** React 19, TypeScript, Tailwind tokens, Radix primitives via `src/components/ui/`, lucide-react, Vitest + Testing Library + user-event, Biome, Yarn.

**Spec:** `docs/superpowers/specs/2026-10-14-ai-agents-dialog-redesign-design.md` (index: `docs/superpowers/plans/2026-10-14-ai-agents-dialog-redesign/00-plan-index.md`).

---

## Global Constraints

Copied from the plan index and `CLAUDE.md`. They apply to every task.

- Hard rules from `CLAUDE.md`: shadcn primitives only (no raw `button`, `input`, `dialog`, `select`, `form`), `lucide-react` icons only (no inline SVGs in components), narrow Zustand selectors, and in production Rust no panicking unwrap calls (map errors explicitly).
- Follow the Rocket DSM. Tokens only, no hard-coded colours. Every new component gets a DSM entry in the plan that builds it.
- Commits: conventional commit format, through the `dev-workflow-skills:1-git-commit` skill, with a pathspec (`git add <paths>` and `git commit --only -- <paths>`). Never `git add -A`, `--all` or `.`.
- Checks: `yarn tsc --noEmit`, `yarn check`, `yarn test <pattern>`. Never `cargo test --workspace`.
- No OpenCollection pre-read is needed: this plan touches no collection, environment or request data.
- Persisted data is unchanged: `CredentialSource` stays `vault | env | keychain` and the `vault_*` fields stay.
- A repo hook blocks writes whose content contains the literal Rust panic call named in the first rule. Do not type that literal in any file.
- The interface contract in `00-plan-index.md` is fixed. Use the exact names, props and file names below. Do not rename.
- Code comments are short, easy, full sentences that end with a punctuation mark.
- Zustand: not used in this plan. Components are pure props in, callbacks out.
- Multi-line editors stay Monaco and single-line variable-aware fields stay `SingleLineEditor`. None of the new components needs either: they use the plain `Input` primitive for short literal text.

## Review Focus

Five inputs most likely to break these components. Each is pinned by a test in the task that owns the component.

1. **Empty rows (`KeyValueRows`, Task 3):** `rows=[]` renders no header and no grid, only the left-aligned Add button, and Add appends exactly one row with a fresh unique key. Pinned by `renders only the Add button for empty rows`.
2. **Very long help text and error text (`FormField`, Task 1; `StateBlock`, Task 1):** a 400-character unbroken string must wrap (`break-words`) and never be truncated or clipped. Pinned by `wraps a very long unbroken help string` and `wraps a very long message`.
3. **Keyboard arrow navigation (`SegmentedControl`, Task 2):** ArrowRight and ArrowLeft move selection and call `onValueChange`, with wrap-around at both ends, and Tab leaves the group in one stop. Pinned by `moves selection with arrow keys and wraps`.
4. **Duplicate ids (`FormField`, Task 1; `KeyValueRows`, Task 3):** two fields with different `id` props produce only unique ids (`-help`, `-error` suffixes), and every row in `KeyValueRows` gets its own id derived from `useId()` plus the stable row key. Pinned by `never emits duplicate ids` and `gives every row control a unique id`.
5. **Reduced motion (`StateBlock` pulse, `CollapsibleSection` chevron, `SegmentedControl` colour transition, Tasks 1 and 2):** every animated or transitioned class carries a `motion-reduce:` variant. Pinned by `disables the pulse for reduced motion`, `disables the chevron transition for reduced motion` and `disables the colour transition for reduced motion`.

---

### Task 1: FormField and StateBlock

**Files:**
- Create: `src/components/ui/form-field.tsx`
- Create: `src/components/ui/state-block.tsx`
- Create: `src/components/ui/__tests__/form-field.test.tsx`
- Create: `src/components/ui/__tests__/state-block.test.tsx`
- Modify: `docs/dsm/index.html`
  - CSS: insert immediately before line 1329 (`/* ─── overlays (DSM sections) ─── */`).
  - Nav: add ONE new nav group, `Form building blocks`, after the last existing group (`Reference`). Insert it immediately after the `href="#sync"` nav item (the last item, just before `</nav>`).
  - Sections: add the new sections at the end of the content, immediately before the line `<div style="height:48px"></div>` (after the `sync` section). The page numbers sections one by one as `NN — Group`; the last existing section is `36 — Reference`, so the new group takes `37` onward.

**Interfaces:**

Consumes:
- `Label` from `@/components/ui/label` (`React.ComponentProps<typeof LabelPrimitive.Root>`, supports `htmlFor`).
- `Button` from `@/components/ui/button` (`variant`, `size`, `onClick`).
- `cn` from `@/lib/utils`.

Produces (exact contract):

```ts
// form-field.tsx
export interface FieldA11yProps {
  id: string;
  'aria-invalid': boolean;
  'aria-describedby': string | undefined;
}
export function FormField(props: {
  id: string;
  label: ReactNode;
  optionalHint?: string;
  group?: boolean;   // true: role="group" + aria-labelledby, label is a span (not a <label>)
  help: ReactNode;
  error?: string | null;
  children: (a11y: FieldA11yProps) => ReactNode;
}): JSX.Element;

// state-block.tsx
export function StateBlock(props: {
  tone: 'loading' | 'empty' | 'error';
  children: ReactNode;
  action?: { label: string; onClick: () => void };
}): JSX.Element;
```

- [ ] **Step 1: Write the failing FormField test**

Create `src/components/ui/__tests__/form-field.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { FormField } from '@/components/ui/form-field';
import { Input } from '@/components/ui/input';

function Demo(props: {
  id?: string;
  error?: string | null;
  help?: string;
  optionalHint?: string;
}) {
  return (
    <FormField
      id={props.id ?? 'demo'}
      label='Idle timeout'
      optionalHint={props.optionalHint}
      help={props.help ?? 'Seconds before an idle session stops.'}
      error={props.error}
    >
      {(a11y) => <Input {...a11y} />}
    </FormField>
  );
}

describe('FormField', () => {
  it('wires the label, the control and the help text together', () => {
    render(<Demo />);
    const input = screen.getByLabelText(/Idle timeout/);
    expect(input).toHaveAttribute('id', 'demo');
    expect(input).toHaveAttribute('aria-invalid', 'false');
    expect(input).toHaveAttribute('aria-describedby', 'demo-help');
    const help = screen.getByText('Seconds before an idle session stops.');
    expect(help).toHaveAttribute('id', 'demo-help');
    expect(help).toHaveClass('text-xs', 'text-muted-foreground');
  });

  it('shows the error with role alert and links it to the control', () => {
    render(<Demo error='Enter a value between 30 and 1800.' />);
    const input = screen.getByLabelText(/Idle timeout/);
    expect(input).toHaveAttribute('aria-invalid', 'true');
    expect(input).toHaveAttribute('aria-describedby', 'demo-help demo-error');
    const alert = screen.getByRole('alert');
    expect(alert).toHaveTextContent('Enter a value between 30 and 1800.');
    expect(alert).toHaveAttribute('id', 'demo-error');
    expect(alert).toHaveClass('text-destructive');
  });

  it('renders no alert for a null or empty error', () => {
    const { rerender } = render(<Demo error={null} />);
    expect(screen.queryByRole('alert')).toBeNull();
    rerender(<Demo error='' />);
    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.getByLabelText(/Idle timeout/)).toHaveAttribute('aria-invalid', 'false');
  });

  it('shows the optional hint beside the label', () => {
    render(<Demo optionalHint='seconds, optional' />);
    expect(screen.getByText('(seconds, optional)')).toBeInTheDocument();
  });

  it('wraps a very long unbroken help string', () => {
    const long = 'x'.repeat(400);
    render(<Demo help={long} />);
    const help = screen.getByText(long);
    expect(help).toHaveClass('break-words');
    expect(help).not.toHaveClass('truncate');
  });

  it('never emits duplicate ids', () => {
    const { container } = render(
      <>
        <Demo id='one' error='Bad.' />
        <Demo id='two' error='Bad.' />
      </>,
    );
    const ids = Array.from(container.querySelectorAll('[id]')).map((node) => node.id);
    expect(ids.length).toBe(6);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it('associates the label with the control through htmlFor in normal mode', () => {
    const { container } = render(<Demo />);
    const label = container.querySelector('label');
    expect(label).not.toBeNull();
    expect(label).toHaveAttribute('for', 'demo');
    expect(screen.queryByRole('group')).toBeNull();
  });

  describe('group mode', () => {
    function GroupDemo(props: { error?: string | null }) {
      return (
        <FormField
          group
          id='grp'
          label='Saved credential'
          help='Stored in the keychain.'
          error={props.error}
        >
          {(a11y) => (
            <div data-testid='child' {...a11y}>
              content
            </div>
          )}
        </FormField>
      );
    }

    it('renders role group named by the label', () => {
      const { container } = render(<GroupDemo />);
      const group = screen.getByRole('group', { name: 'Saved credential' });
      const label = container.querySelector('#grp-label');
      expect(label).not.toBeNull();
      expect(group).toHaveAttribute('aria-labelledby', 'grp-label');
      expect(label?.tagName).toBe('SPAN');
      expect(container.querySelector('label')).toBeNull();
    });

    it('still passes id, aria-invalid and aria-describedby to the child', () => {
      render(<GroupDemo error='Bad.' />);
      const child = screen.getByTestId('child');
      expect(child).toHaveAttribute('id', 'grp');
      expect(child).toHaveAttribute('aria-invalid', 'true');
      expect(child).toHaveAttribute('aria-describedby', 'grp-help grp-error');
    });

    it('never emits duplicate ids', () => {
      const { container } = render(<GroupDemo error='Bad.' />);
      const ids = Array.from(container.querySelectorAll('[id]')).map((node) => node.id);
      expect(new Set(ids).size).toBe(ids.length);
    });
  });
});
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `yarn test src/components/ui/__tests__/form-field.test.tsx`
Expected: FAIL with `Failed to resolve import "@/components/ui/form-field"`.

- [ ] **Step 3: Implement FormField**

Create `src/components/ui/form-field.tsx`:

```tsx
import type { ReactNode } from 'react';
import { Label } from '@/components/ui/label';

export interface FieldA11yProps {
  id: string;
  'aria-invalid': boolean;
  'aria-describedby': string | undefined;
}

export interface FormFieldProps {
  id: string;
  label: ReactNode;
  optionalHint?: string;
  /** True when the label names a group of controls, such as a radio set or a row list. */
  group?: boolean;
  help: ReactNode;
  error?: string | null;
  children: (a11y: FieldA11yProps) => ReactNode;
}

/**
 * Label, control, always-visible help and an optional error.
 * The control comes from the render prop so it gets the id and aria wiring.
 * In group mode the wrapper is a labelled group and the label is not a <label>.
 */
export function FormField({
  id,
  label,
  optionalHint,
  group = false,
  help,
  error,
  children,
}: FormFieldProps) {
  const helpId = `${id}-help`;
  const errorId = `${id}-error`;
  const labelId = `${id}-label`;
  const hasError = Boolean(error);
  const describedBy = hasError ? `${helpId} ${errorId}` : helpId;

  const labelContent = (
    <>
      {label}
      {optionalHint && (
        <span className='ml-1.5 text-xs font-normal text-muted-foreground'>({optionalHint})</span>
      )}
    </>
  );

  return (
    <div
      className='space-y-1.5'
      role={group ? 'group' : undefined}
      aria-labelledby={group ? labelId : undefined}
    >
      {group ? (
        <span id={labelId} className='block text-sm font-medium leading-none'>
          {labelContent}
        </span>
      ) : (
        <Label htmlFor={id}>{labelContent}</Label>
      )}
      {children({ id, 'aria-invalid': hasError, 'aria-describedby': describedBy })}
      <p id={helpId} className='break-words text-xs leading-relaxed text-muted-foreground'>
        {help}
      </p>
      {hasError && (
        <p id={errorId} role='alert' className='break-words text-xs text-destructive'>
          {error}
        </p>
      )}
    </div>
  );
}
```

- [ ] **Step 4: Run the test and confirm it passes**

Run: `yarn test src/components/ui/__tests__/form-field.test.tsx`
Expected: PASS, 10 tests.

- [ ] **Step 5: Write the failing StateBlock test**

Create `src/components/ui/__tests__/state-block.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { StateBlock } from '@/components/ui/state-block';

describe('StateBlock', () => {
  it('uses role status for loading and empty', () => {
    const { rerender } = render(<StateBlock tone='loading'>Loading secrets.</StateBlock>);
    expect(screen.getByRole('status')).toHaveTextContent('Loading secrets.');
    rerender(<StateBlock tone='empty'>No secrets found.</StateBlock>);
    expect(screen.getByRole('status')).toHaveTextContent('No secrets found.');
  });

  it('uses role alert for the error tone', () => {
    render(<StateBlock tone='error'>Could not list secrets.</StateBlock>);
    const block = screen.getByRole('alert');
    expect(block).toHaveTextContent('Could not list secrets.');
    expect(block).toHaveAttribute('data-tone', 'error');
    expect(block).toHaveClass('border-dashed', 'text-xs');
  });

  it('renders a hidden status dot with a tone colour', () => {
    render(<StateBlock tone='error'>Failed.</StateBlock>);
    const dot = screen.getByRole('alert').querySelector('[data-slot="state-dot"]');
    expect(dot).not.toBeNull();
    expect(dot).toHaveAttribute('aria-hidden', 'true');
    expect(dot).toHaveClass('bg-destructive');
  });

  it('runs the action when its button is clicked', async () => {
    const onClick = vi.fn();
    render(
      <StateBlock tone='error' action={{ label: 'Retry', onClick }}>
        Failed.
      </StateBlock>,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Retry' }));
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it('renders no button without an action', () => {
    render(<StateBlock tone='empty'>Nothing here.</StateBlock>);
    expect(screen.queryByRole('button')).toBeNull();
  });

  it('wraps a very long message', () => {
    const long = 'y'.repeat(400);
    render(<StateBlock tone='error'>{long}</StateBlock>);
    expect(screen.getByText(long)).toHaveClass('break-words');
  });

  it('disables the pulse for reduced motion', () => {
    render(<StateBlock tone='loading'>Loading.</StateBlock>);
    const dot = screen.getByRole('status').querySelector('[data-slot="state-dot"]');
    expect(dot).toHaveClass('animate-pulse', 'motion-reduce:animate-none');
  });
});
```

- [ ] **Step 6: Run it and confirm it fails**

Run: `yarn test src/components/ui/__tests__/state-block.test.tsx`
Expected: FAIL with `Failed to resolve import "@/components/ui/state-block"`.

- [ ] **Step 7: Implement StateBlock**

Create `src/components/ui/state-block.tsx`:

```tsx
import type { ReactNode } from 'react';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

export type StateBlockTone = 'loading' | 'empty' | 'error';

export interface StateBlockProps {
  tone: StateBlockTone;
  children: ReactNode;
  action?: { label: string; onClick: () => void };
}

const DOT_CLASS: Record<StateBlockTone, string> = {
  loading: 'bg-muted-foreground animate-pulse motion-reduce:animate-none',
  empty: 'bg-muted-foreground/50',
  error: 'bg-destructive',
};

/** Inline dashed block for a loading, empty or error state, with an optional action. */
export function StateBlock({ tone, children, action }: StateBlockProps) {
  return (
    <div
      role={tone === 'error' ? 'alert' : 'status'}
      data-slot='state-block'
      data-tone={tone}
      className={cn(
        'flex items-center gap-2 rounded-md border border-dashed px-3 py-2 text-xs text-muted-foreground',
        tone === 'error' && 'border-destructive/50',
      )}
    >
      <span
        data-slot='state-dot'
        aria-hidden='true'
        className={cn('h-2 w-2 shrink-0 rounded-full', DOT_CLASS[tone])}
      />
      <span className='min-w-0 flex-1 break-words'>{children}</span>
      {action && (
        <Button variant='outline' size='sm' className='h-7 shrink-0' onClick={action.onClick}>
          {action.label}
        </Button>
      )}
    </div>
  );
}
```

- [ ] **Step 8: Run both tests and confirm they pass**

Run: `yarn test src/components/ui/__tests__/form-field.test.tsx src/components/ui/__tests__/state-block.test.tsx`
Expected: PASS, 17 tests.

- [ ] **Step 9: Add the DSM entries (CSS, nav, sections)**

In `docs/dsm/index.html`, with the Edit tool:

(a) CSS. Replace the line `/* ─── overlays (DSM sections) ─── */` with the block below followed by that same line:

```css
/* ── Form field (ui/form-field.tsx) ── */
.c-ff { display: flex; flex-direction: column; gap: 6px; width: 320px; max-width: 100%; }
.c-ff-opt { margin-left: 6px; font-size: 12px; font-weight: 400; color: var(--muted-foreground); }
.c-ff-help { margin: 0; font-size: 12px; line-height: 1.625; color: var(--muted-foreground); overflow-wrap: break-word; }
.c-ff-err { margin: 0; font-size: 12px; line-height: 1.625; color: var(--destructive); overflow-wrap: break-word; }

/* ── State block (ui/state-block.tsx) ── */
.c-sb { display: flex; align-items: center; gap: 8px; width: 320px; max-width: 100%; box-sizing: border-box; padding: 8px 12px; border: 1px dashed var(--border); border-radius: var(--radius-md); font-size: 12px; line-height: 16px; color: var(--muted-foreground); }
.c-sb.is-error { border-color: color-mix(in srgb, var(--destructive) 50%, transparent); }
.c-sb-dot { width: 8px; height: 8px; flex-shrink: 0; border-radius: 9999px; background: var(--muted-foreground); }
.c-sb-dot.is-loading { animation: c-sb-pulse 2s cubic-bezier(0.4, 0, 0.6, 1) infinite; }
.c-sb-dot.is-empty { background: color-mix(in srgb, var(--muted-foreground) 50%, transparent); }
.c-sb-dot.is-error { background: var(--destructive); }
.c-sb-text { min-width: 0; flex: 1; overflow-wrap: break-word; }
@keyframes c-sb-pulse { 50% { opacity: 0.5; } }
@media (prefers-reduced-motion: reduce) { .c-sb-dot.is-loading { animation: none; } }
```

(b) Nav. The page's sidebar has groups (`Foundations`, `Components`, `Patterns`, `Reference`). Add one new group after `Reference`. Replace the `href="#sync"` nav line with itself followed by the lines below (the `</nav>` that follows stays untouched):

```html

      <div class="nav-section" style="margin-top:10px">Form building blocks</div>
      <a class="nav-item" href="#form-field" onclick="scrollTo('form-field',this)"><span class="dot"></span> Form field</a>
      <a class="nav-item" href="#state-block" onclick="scrollTo('state-block',this)"><span class="dot"></span> State block</a>
```

(c) Sections. Replace the line `<div style="height:48px"></div>` with the two sections below followed by that same line. Keep the blank line between sections:

```html
<section class="dsm-section" id="form-field">
        <div class="section-title">37 — Form building blocks</div>
        <div class="section-heading">Form field</div>
        <p class="section-desc">Label, control, always-visible help and an optional error, wired with <code class="o-code">htmlFor</code>, <code class="o-code">aria-invalid</code> and <code class="o-code">aria-describedby</code>. Pass <code class="o-code">group</code> when the label names a set of controls (a radio set, a row list or a saved secret): the wrapper then becomes a labelled group and the label is a span, not a label element. The <code class="o-code">help</code> prop is required, so a field cannot ship without it. Write the NAME of a variable in capitals when it is a name and not a value.</p>
<div class="dsm-card"><div class="component-label">Help only, optional hint, and error</div><div class="component-row" style="align-items:flex-start"><div class="c-ff"><label class="c-label" for="ff-a">Label</label><input id="ff-a" class="c-input" type="text" value="My agent" aria-describedby="ff-a-help"><p id="ff-a-help" class="c-ff-help">A name for this agent in lists. Example: Gemini work account.</p></div><div class="c-ff"><label class="c-label" for="ff-b">Idle timeout<span class="c-ff-opt">(seconds, optional)</span></label><input id="ff-b" class="c-input is-invalid" type="text" value="5" aria-invalid="true" aria-describedby="ff-b-help ff-b-error"><p id="ff-b-help" class="c-ff-help">Seconds before an idle session stops. Leave empty for the default.</p><p id="ff-b-error" class="c-ff-err" role="alert">Enter a value between 30 and 1800.</p></div></div><div class="component-label" style="margin-top:16px">Spec</div><div class="c-spec-wrap"><table class="c-spec"><thead><tr><th>Part</th><th>Tailwind classes</th><th>Source</th></tr></thead><tbody><tr><td>Wrapper</td><td>space-y-1.5</td><td>form-field.tsx:34</td></tr><tr><td>Label</td><td>Label primitive (text-sm font-medium leading-none) with htmlFor = id</td><td>form-field.tsx:35</td></tr><tr><td>Optional hint</td><td>ml-1.5 text-xs font-normal text-muted-foreground</td><td>form-field.tsx:37</td></tr><tr><td>Help</td><td>id = {id}-help; break-words text-xs leading-relaxed text-muted-foreground</td><td>form-field.tsx:43</td></tr><tr><td>Error</td><td>id = {id}-error; role="alert"; break-words text-xs text-destructive</td><td>form-field.tsx:47</td></tr><tr><td>Control wiring</td><td>children receives { id, aria-invalid, aria-describedby } (help, plus error when set)</td><td>form-field.tsx:41</td></tr><tr><td>Group mode</td><td>group prop: wrapper gets role="group" and aria-labelledby = {id}-label; the label is a span (block text-sm font-medium leading-none), not a label element. Use it for a radio set, a row list or a saved secret.</td><td>form-field.tsx:44</td></tr></tbody></table></div></div>
      </section>

<section class="dsm-section" id="state-block">
        <div class="section-title">38 — Form building blocks</div>
        <div class="section-heading">State block</div>
        <p class="section-desc">Inline dashed block for a loading, empty or error state inside a form. Loading and empty use <code class="o-code">role="status"</code>, error uses <code class="o-code">role="alert"</code>. An optional action renders an outline small Button, for example Retry.</p>
<div class="dsm-card"><div class="component-label">Loading, empty and error with action</div><div class="c-stack"><div class="c-sb" role="status"><span class="c-sb-dot is-loading"></span><span class="c-sb-text">Loading secrets.</span></div><div class="c-sb" role="status"><span class="c-sb-dot is-empty"></span><span class="c-sb-text">No secrets in this connection.</span></div><div class="c-sb is-error" role="alert"><span class="c-sb-dot is-error"></span><span class="c-sb-text">Could not list secrets.</span><span class="c-btn c-btn-outline c-btn-sm" style="height:28px">Retry</span></div></div><div class="component-label" style="margin-top:16px">Spec</div><div class="c-spec-wrap"><table class="c-spec"><thead><tr><th>Part</th><th>Tailwind classes</th><th>Source</th></tr></thead><tbody><tr><td>Block</td><td>flex items-center gap-2 rounded-md border border-dashed px-3 py-2 text-xs text-muted-foreground; error adds border-destructive/50</td><td>state-block.tsx:30</td></tr><tr><td>Dot</td><td>h-2 w-2 shrink-0 rounded-full; loading bg-muted-foreground animate-pulse motion-reduce:animate-none; empty bg-muted-foreground/50; error bg-destructive</td><td>state-block.tsx:16-20</td></tr><tr><td>Message</td><td>min-w-0 flex-1 break-words</td><td>state-block.tsx:45</td></tr><tr><td>Action</td><td>Button outline sm, h-7 shrink-0</td><td>state-block.tsx:47</td></tr></tbody></table></div></div>
      </section>

<div style="height:48px"></div>
```

Note: the `source` line numbers in the spec tables are approximate. After the implementation files are final, adjust them to the real line numbers.

- [ ] **Step 10: Run the type check and the linter**

Run: `yarn tsc --noEmit`
Expected: exits 0, no errors.

Run: `yarn check`
Expected: exits 0. If Biome reports formatting in the new files, run `yarn format` and re-run `yarn check`.

- [ ] **Step 11: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill and commit with a pathspec. Suggested message: `feat(ui): add FormField and StateBlock components with DSM entries`.

```bash
git add src/components/ui/form-field.tsx src/components/ui/state-block.tsx \
  src/components/ui/__tests__/form-field.test.tsx src/components/ui/__tests__/state-block.test.tsx \
  docs/dsm/index.html
git commit --only -m "feat(ui): add FormField and StateBlock components with DSM entries" -- \
  src/components/ui/form-field.tsx src/components/ui/state-block.tsx \
  src/components/ui/__tests__/form-field.test.tsx src/components/ui/__tests__/state-block.test.tsx \
  docs/dsm/index.html
```

---

### Task 2: SegmentedControl and CollapsibleSection

**Files:**
- Create: `src/components/ui/segmented-control.tsx`
- Create: `src/components/ui/collapsible-section.tsx`
- Create: `src/components/ui/__tests__/segmented-control.test.tsx`
- Create: `src/components/ui/__tests__/collapsible-section.test.tsx`
- Modify: `docs/dsm/index.html`
  - CSS: insert immediately before the line `/* ─── overlays (DSM sections) ─── */`.
  - Nav: insert immediately after the nav item added by the previous task (the `href="#state-block"` item), inside the `Form building blocks` group.
  - Sections: insert immediately before the line `<div style="height:48px"></div>` (after the section added by the previous task).

**Interfaces:**

Consumes:
- `@radix-ui/react-radio-group` (`Root`, `Item`), the same primitive that `src/components/ui/radio-group.tsx` wraps. `RadioGroupItem` in `ui/radio-group.tsx` always renders a circle indicator child and cannot show text, so the segmented control uses the Radix parts directly. It lives in `ui/`, like `radio-group.tsx`, so this stays inside the primitive layer.
- `Collapsible`, `CollapsibleTrigger`, `CollapsibleContent` from `@/components/ui/collapsible`.
- `Button` from `@/components/ui/button`; `ChevronDown` from `lucide-react`; `cn` from `@/lib/utils`.

Produces (exact contract):

```ts
// segmented-control.tsx
export function SegmentedControl<T extends string>(props: {
  value: T;
  onValueChange: (value: T) => void;
  options: { value: T; label: string }[];
  'aria-label': string;
}): JSX.Element;

// collapsible-section.tsx
export function CollapsibleSection(props: {
  title: string;
  summary?: string;
  defaultOpen?: boolean;   // default false (uncontrolled)
  open?: boolean;          // when given, the section is controlled
  onOpenChange?: (open: boolean) => void;
  children: ReactNode;
}): JSX.Element;
```

- [ ] **Step 1: Write the failing SegmentedControl test**

Create `src/components/ui/__tests__/segmented-control.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { SegmentedControl } from '@/components/ui/segmented-control';

type Source = 'vault' | 'env' | 'keychain';

const OPTIONS: { value: Source; label: string }[] = [
  { value: 'vault', label: 'Secret manager' },
  { value: 'env', label: 'Environment' },
  { value: 'keychain', label: 'Keychain' },
];

function Controlled({ onValue }: { onValue?: (value: Source) => void }) {
  const [value, setValue] = useState<Source>('vault');
  return (
    <SegmentedControl<Source>
      aria-label='Credential source'
      value={value}
      options={OPTIONS}
      onValueChange={(next) => {
        setValue(next);
        onValue?.(next);
      }}
    />
  );
}

describe('SegmentedControl', () => {
  it('renders a labelled radio group with the selected option checked', () => {
    render(<Controlled />);
    expect(screen.getByRole('radiogroup', { name: 'Credential source' })).toBeInTheDocument();
    expect(screen.getAllByRole('radio')).toHaveLength(3);
    expect(screen.getByRole('radio', { name: 'Secret manager' })).toBeChecked();
    expect(screen.getByRole('radio', { name: 'Keychain' })).not.toBeChecked();
  });

  it('selects an option on click', async () => {
    const onValue = vi.fn();
    render(<Controlled onValue={onValue} />);
    await userEvent.click(screen.getByRole('radio', { name: 'Environment' }));
    expect(onValue).toHaveBeenLastCalledWith('env');
    expect(screen.getByRole('radio', { name: 'Environment' })).toBeChecked();
  });

  it('moves selection with arrow keys and wraps', async () => {
    const onValue = vi.fn();
    render(<Controlled onValue={onValue} />);
    screen.getByRole('radio', { name: 'Secret manager' }).focus();

    await userEvent.keyboard('{ArrowRight}');
    expect(onValue).toHaveBeenLastCalledWith('env');
    await userEvent.keyboard('{ArrowRight}');
    expect(onValue).toHaveBeenLastCalledWith('keychain');
    await userEvent.keyboard('{ArrowRight}');
    expect(onValue).toHaveBeenLastCalledWith('vault');
    await userEvent.keyboard('{ArrowLeft}');
    expect(onValue).toHaveBeenLastCalledWith('keychain');
  });

  it('styles the track and the selected segment with tokens', () => {
    render(<Controlled />);
    expect(screen.getByRole('radiogroup')).toHaveClass('bg-muted', 'rounded-md');
    expect(screen.getByRole('radio', { name: 'Secret manager' })).toHaveClass(
      'data-[state=checked]:bg-background',
      'data-[state=checked]:text-foreground',
    );
  });

  it('disables the colour transition for reduced motion', () => {
    render(<Controlled />);
    expect(screen.getByRole('radio', { name: 'Environment' })).toHaveClass(
      'transition-colors',
      'motion-reduce:transition-none',
    );
  });
});
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `yarn test src/components/ui/__tests__/segmented-control.test.tsx`
Expected: FAIL with `Failed to resolve import "@/components/ui/segmented-control"`.

- [ ] **Step 3: Implement SegmentedControl**

Create `src/components/ui/segmented-control.tsx`:

```tsx
import * as RadioGroupPrimitive from '@radix-ui/react-radio-group';

export interface SegmentedControlProps<T extends string> {
  value: T;
  onValueChange: (value: T) => void;
  options: { value: T; label: string }[];
  'aria-label': string;
}

/** A radio group drawn as a segmented track. Arrow keys move and select. */
export function SegmentedControl<T extends string>({
  value,
  onValueChange,
  options,
  'aria-label': ariaLabel,
}: SegmentedControlProps<T>) {
  return (
    <RadioGroupPrimitive.Root
      value={value}
      onValueChange={(next) => onValueChange(next as T)}
      aria-label={ariaLabel}
      orientation='horizontal'
      className='inline-flex w-fit gap-0.5 rounded-md bg-muted p-0.5'
    >
      {options.map((option) => (
        <RadioGroupPrimitive.Item
          key={option.value}
          value={option.value}
          className='rounded-sm px-3 py-1 text-xs font-medium text-muted-foreground outline-none transition-colors hover:text-foreground focus-visible:ring-[3px] focus-visible:ring-ring/50 motion-reduce:transition-none disabled:cursor-not-allowed disabled:opacity-50 data-[state=checked]:bg-background data-[state=checked]:text-foreground data-[state=checked]:shadow-xs'
        >
          {option.label}
        </RadioGroupPrimitive.Item>
      ))}
    </RadioGroupPrimitive.Root>
  );
}
```

- [ ] **Step 4: Run the test and confirm it passes**

Run: `yarn test src/components/ui/__tests__/segmented-control.test.tsx`
Expected: PASS, 5 tests. If the arrow-key test fails because focus did not move into the group, the cause is the test harness, not the component. Keep `.focus()` on the checked radio before pressing keys.

- [ ] **Step 5: Write the failing CollapsibleSection test**

Create `src/components/ui/__tests__/collapsible-section.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { CollapsibleSection } from '@/components/ui/collapsible-section';

describe('CollapsibleSection', () => {
  it('is closed by default and shows the title and summary', () => {
    render(
      <CollapsibleSection title='Advanced' summary='Idle 300s, 2 variables'>
        <p>Inner content</p>
      </CollapsibleSection>,
    );
    const trigger = screen.getByRole('button', { name: /Advanced/ });
    expect(trigger).toHaveAttribute('aria-expanded', 'false');
    expect(screen.getByText('Idle 300s, 2 variables')).toBeInTheDocument();
    expect(screen.queryByText('Inner content')).toBeNull();
  });

  it('opens and closes when the trigger is clicked', async () => {
    render(
      <CollapsibleSection title='Advanced'>
        <p>Inner content</p>
      </CollapsibleSection>,
    );
    const trigger = screen.getByRole('button', { name: /Advanced/ });
    await userEvent.click(trigger);
    expect(trigger).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByText('Inner content')).toBeInTheDocument();
    await userEvent.click(trigger);
    expect(screen.queryByText('Inner content')).toBeNull();
  });

  it('opens with the keyboard', async () => {
    render(
      <CollapsibleSection title='Advanced'>
        <p>Inner content</p>
      </CollapsibleSection>,
    );
    screen.getByRole('button', { name: /Advanced/ }).focus();
    await userEvent.keyboard('{Enter}');
    expect(screen.getByText('Inner content')).toBeInTheDocument();
  });

  it('honours defaultOpen', () => {
    render(
      <CollapsibleSection title='Advanced' defaultOpen>
        <p>Inner content</p>
      </CollapsibleSection>,
    );
    expect(screen.getByRole('button', { name: /Advanced/ })).toHaveAttribute(
      'aria-expanded',
      'true',
    );
    expect(screen.getByText('Inner content')).toBeInTheDocument();
  });

  it('follows the open prop when controlled and reports toggles', async () => {
    const onOpenChange = vi.fn();
    const { rerender } = render(
      <CollapsibleSection title='Advanced' open={false} onOpenChange={onOpenChange}>
        <p>Inner content</p>
      </CollapsibleSection>,
    );
    const trigger = screen.getByRole('button', { name: /Advanced/ });
    expect(trigger).toHaveAttribute('aria-expanded', 'false');
    await userEvent.click(trigger);
    expect(onOpenChange).toHaveBeenLastCalledWith(true);
    // The parent did not change the prop, so the section stays closed.
    expect(trigger).toHaveAttribute('aria-expanded', 'false');
    expect(screen.queryByText('Inner content')).toBeNull();

    rerender(
      <CollapsibleSection title='Advanced' open onOpenChange={onOpenChange}>
        <p>Inner content</p>
      </CollapsibleSection>,
    );
    expect(trigger).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByText('Inner content')).toBeInTheDocument();
    await userEvent.click(trigger);
    expect(onOpenChange).toHaveBeenLastCalledWith(false);
  });

  it('still honours defaultOpen when uncontrolled and reports toggles', async () => {
    const onOpenChange = vi.fn();
    render(
      <CollapsibleSection title='Advanced' defaultOpen onOpenChange={onOpenChange}>
        <p>Inner content</p>
      </CollapsibleSection>,
    );
    const trigger = screen.getByRole('button', { name: /Advanced/ });
    expect(screen.getByText('Inner content')).toBeInTheDocument();
    await userEvent.click(trigger);
    expect(onOpenChange).toHaveBeenLastCalledWith(false);
    expect(screen.queryByText('Inner content')).toBeNull();
  });

  it('does not mount the children while closed', () => {
    let mounts = 0;
    function Probe() {
      mounts += 1;
      return <p>Probe content</p>;
    }
    const { rerender } = render(
      <CollapsibleSection title='Advanced'>
        <Probe />
      </CollapsibleSection>,
    );
    expect(mounts).toBe(0);
    expect(screen.queryByText('Probe content')).toBeNull();
    rerender(
      <CollapsibleSection title='Advanced' open>
        <Probe />
      </CollapsibleSection>,
    );
    expect(mounts).toBeGreaterThan(0);
    expect(screen.getByText('Probe content')).toBeInTheDocument();
  });

  it('keeps a very long summary on one line with an ellipsis', () => {
    const long = 'z'.repeat(300);
    render(
      <CollapsibleSection title='Advanced' summary={long}>
        <p>Inner content</p>
      </CollapsibleSection>,
    );
    expect(screen.getByText(long)).toHaveClass('truncate', 'min-w-0');
  });

  it('disables the chevron transition for reduced motion', () => {
    render(
      <CollapsibleSection title='Advanced'>
        <p>Inner content</p>
      </CollapsibleSection>,
    );
    const chevron = screen.getByRole('button', { name: /Advanced/ }).querySelector('svg');
    expect(chevron).toHaveClass('transition-transform', 'motion-reduce:transition-none');
    expect(chevron).toHaveAttribute('aria-hidden', 'true');
  });
});
```

- [ ] **Step 6: Run it and confirm it fails**

Run: `yarn test src/components/ui/__tests__/collapsible-section.test.tsx`
Expected: FAIL with `Failed to resolve import "@/components/ui/collapsible-section"`.

- [ ] **Step 7: Implement CollapsibleSection**

Create `src/components/ui/collapsible-section.tsx`:

```tsx
import { ChevronDown } from 'lucide-react';
import type { ReactNode } from 'react';
import { Button } from '@/components/ui/button';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible';

export interface CollapsibleSectionProps {
  title: string;
  summary?: string;
  /** Initial state when uncontrolled. */
  defaultOpen?: boolean;
  /** When given, the section is controlled and `defaultOpen` is ignored. */
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  children: ReactNode;
}

/**
 * A collapsible group. The header row shows the title and a one-line summary.
 * A parent can open it, for example to show an error, through `open` and `onOpenChange`.
 */
export function CollapsibleSection({
  title,
  summary,
  defaultOpen = false,
  open,
  onOpenChange,
  children,
}: CollapsibleSectionProps) {
  const stateProps =
    open === undefined ? { defaultOpen, onOpenChange } : { open, onOpenChange };
  return (
    <Collapsible {...stateProps} className='rounded-md border'>
      <CollapsibleTrigger asChild>
        <Button
          variant='ghost'
          className='group h-auto w-full justify-start gap-2 rounded-b-none px-3 py-2 text-left'
        >
          <ChevronDown
            aria-hidden='true'
            className='h-4 w-4 shrink-0 text-muted-foreground transition-transform motion-reduce:transition-none group-data-[state=open]:rotate-180'
          />
          <span className='text-sm font-medium'>{title}</span>
          {summary && (
            <span className='min-w-0 flex-1 truncate text-xs font-normal text-muted-foreground'>
              {summary}
            </span>
          )}
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent className='space-y-4 border-t px-3 py-3'>{children}</CollapsibleContent>
    </Collapsible>
  );
}
```

- [ ] **Step 8: Run both tests and confirm they pass**

Run: `yarn test src/components/ui/__tests__/segmented-control.test.tsx src/components/ui/__tests__/collapsible-section.test.tsx`
Expected: PASS, 14 tests.

- [ ] **Step 9: Add the DSM entries (CSS, nav, sections)**

In `docs/dsm/index.html`, with the Edit tool:

(a) CSS. Replace the line `/* ─── overlays (DSM sections) ─── */` with the block below followed by that same line:

```css
/* ── Segmented control (ui/segmented-control.tsx) ── */
.c-seg { display: inline-flex; gap: 2px; padding: 2px; border-radius: var(--radius-md); background: var(--muted); }
.c-seg > span { padding: 4px 12px; border-radius: 4px; font-size: 12px; line-height: 16px; font-weight: 500; color: var(--muted-foreground); }
.c-seg > span.is-on { background: var(--background); color: var(--foreground); box-shadow: 0 1px 2px 0 rgb(0 0 0 / 0.05); }
.c-seg > span.is-focus { box-shadow: 0 0 0 3px color-mix(in srgb, var(--ring) 50%, transparent); }

/* ── Collapsible section (ui/collapsible-section.tsx) ── */
.c-cs { width: 360px; max-width: 100%; border: 1px solid var(--border); border-radius: var(--radius-md); }
.c-cs-head { display: flex; align-items: center; gap: 8px; padding: 8px 12px; }
.c-cs-head svg { width: 16px; height: 16px; flex-shrink: 0; color: var(--muted-foreground); transition: transform 0.15s; }
.c-cs.is-open .c-cs-head svg { transform: rotate(180deg); }
.c-cs-title { font-size: 14px; font-weight: 500; color: var(--foreground); }
.c-cs-sum { min-width: 0; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; color: var(--muted-foreground); }
.c-cs-body { padding: 12px; border-top: 1px solid var(--border); font-size: 12px; color: var(--muted-foreground); }
@media (prefers-reduced-motion: reduce) { .c-cs-head svg { transition: none; } }
```

(b) Nav. Replace the `href="#state-block"` nav line (added by Task 1) with itself followed by these two lines:

```html
      <a class="nav-item" href="#segmented-control" onclick="scrollTo('segmented-control',this)"><span class="dot"></span> Segmented control</a>
      <a class="nav-item" href="#collapsible-section" onclick="scrollTo('collapsible-section',this)"><span class="dot"></span> Collapsible section</a>
```

(c) Sections. Replace the line `<div style="height:48px"></div>` with the two sections below followed by that same line:

```html
<section class="dsm-section" id="segmented-control">
        <div class="section-title">39 — Form building blocks</div>
        <div class="section-heading">Segmented control</div>
        <p class="section-desc">A Radix radio group drawn as a segmented track, for a short either-or choice such as the credential source. It is one tab stop. Arrow keys move and select, with wrap-around.</p>
<div class="dsm-card"><div class="component-label">Selected, and keyboard focus on a segment</div><div class="c-stack"><div class="c-seg" role="radiogroup" aria-label="Credential source"><span class="is-on" role="radio" aria-checked="true">Secret manager</span><span role="radio" aria-checked="false">Environment</span><span role="radio" aria-checked="false">Keychain</span></div><div class="c-seg" role="radiogroup" aria-label="Credential source (focus)"><span role="radio" aria-checked="false">Secret manager</span><span class="is-on is-focus" role="radio" aria-checked="true">Environment</span><span role="radio" aria-checked="false">Keychain</span></div></div><div class="component-label" style="margin-top:16px">Spec</div><div class="c-spec-wrap"><table class="c-spec"><thead><tr><th>Part</th><th>Tailwind classes</th><th>Source</th></tr></thead><tbody><tr><td>Track</td><td>inline-flex w-fit gap-0.5 rounded-md bg-muted p-0.5</td><td>segmented-control.tsx:24</td></tr><tr><td>Segment</td><td>rounded-sm px-3 py-1 text-xs font-medium text-muted-foreground transition-colors motion-reduce:transition-none</td><td>segmented-control.tsx:30</td></tr><tr><td>Selected</td><td>data-[state=checked]:bg-background data-[state=checked]:text-foreground data-[state=checked]:shadow-xs</td><td>segmented-control.tsx:30</td></tr><tr><td>Focus</td><td>focus-visible:ring-[3px] focus-visible:ring-ring/50</td><td>segmented-control.tsx:30</td></tr><tr><td>Behaviour</td><td>Radix RadioGroup, horizontal, roving tab stop, arrow keys select and wrap</td><td>segmented-control.tsx:21</td></tr></tbody></table></div></div>
      </section>

<section class="dsm-section" id="collapsible-section">
        <div class="section-title">40 — Form building blocks</div>
        <div class="section-heading">Collapsible section</div>
        <p class="section-desc">A bordered group built on the Collapsible primitive. The header row shows the title and a one-line summary, so a closed section still tells the user what is inside. The chevron turns when open. Pass <code class="o-code">open</code> and <code class="o-code">onOpenChange</code> to control it, for example to open it when a field inside has an error.</p>
<div class="dsm-card"><div class="component-label">Closed with a summary, and open</div><div class="c-stack"><div class="c-cs"><div class="c-cs-head"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg><span class="c-cs-title">Advanced</span><span class="c-cs-sum">Idle 300s, 2 variables</span></div></div><div class="c-cs is-open"><div class="c-cs-head"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg><span class="c-cs-title">Advanced</span></div><div class="c-cs-body">Idle timeout, reasoning and extra environment variables go here.</div></div></div><div class="component-label" style="margin-top:16px">Spec</div><div class="c-spec-wrap"><table class="c-spec"><thead><tr><th>Part</th><th>Tailwind classes</th><th>Source</th></tr></thead><tbody><tr><td>Group</td><td>rounded-md border (Collapsible root)</td><td>collapsible-section.tsx:22</td></tr><tr><td>Header</td><td>ghost Button, group h-auto w-full justify-start gap-2 rounded-b-none px-3 py-2 text-left</td><td>collapsible-section.tsx:24</td></tr><tr><td>Chevron</td><td>h-4 w-4 shrink-0 text-muted-foreground transition-transform motion-reduce:transition-none group-data-[state=open]:rotate-180</td><td>collapsible-section.tsx:29</td></tr><tr><td>Title</td><td>text-sm font-medium</td><td>collapsible-section.tsx:33</td></tr><tr><td>Summary</td><td>min-w-0 flex-1 truncate text-xs font-normal text-muted-foreground</td><td>collapsible-section.tsx:35</td></tr><tr><td>Body</td><td>space-y-4 border-t px-3 py-3</td><td>collapsible-section.tsx:41</td></tr><tr><td>Default</td><td>closed; defaultOpen opens it when uncontrolled</td><td>collapsible-section.tsx:16</td></tr><tr><td>Controlled</td><td>open and onOpenChange make it controlled, so a parent can open it to show an error. Children are not mounted while closed.</td><td>collapsible-section.tsx:31</td></tr></tbody></table></div></div>
      </section>

<div style="height:48px"></div>
```

Note: the line numbers in the spec tables are approximate. After the implementation files are final, adjust them to the real line numbers.

- [ ] **Step 10: Run the type check and the linter**

Run: `yarn tsc --noEmit`
Expected: exits 0, no errors.

Run: `yarn check`
Expected: exits 0. If Biome reports formatting in the new files, run `yarn format` and re-run `yarn check`.

- [ ] **Step 11: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill and commit with a pathspec. Suggested message: `feat(ui): add SegmentedControl and CollapsibleSection components with DSM entries`.

```bash
git add src/components/ui/segmented-control.tsx src/components/ui/collapsible-section.tsx \
  src/components/ui/__tests__/segmented-control.test.tsx src/components/ui/__tests__/collapsible-section.test.tsx \
  docs/dsm/index.html
git commit --only -m "feat(ui): add SegmentedControl and CollapsibleSection components with DSM entries" -- \
  src/components/ui/segmented-control.tsx src/components/ui/collapsible-section.tsx \
  src/components/ui/__tests__/segmented-control.test.tsx src/components/ui/__tests__/collapsible-section.test.tsx \
  docs/dsm/index.html
```

---

### Task 3: SecretField, KeyValueRows and DialogBody

**Files:**
- Create: `src/components/ui/secret-field.tsx`
- Create: `src/components/ui/key-value-rows.tsx`
- Create: `src/components/ui/__tests__/secret-field.test.tsx`
- Create: `src/components/ui/__tests__/key-value-rows.test.tsx`
- Create: `src/components/ui/__tests__/dialog-body.test.tsx`
- Modify: `src/components/ui/saved-pill.tsx` lines 1-24 (add a `persistent` mode and a `children` label; default behaviour unchanged).
- Modify: `src/components/ui/dialog.tsx`: add `DialogBody` after `DialogFooter` (after line 73) and add `DialogBody` to the export list (lines 97-108).
- Modify: `docs/dsm/index.html`
  - CSS: insert immediately before the line `/* ─── overlays (DSM sections) ─── */`.
  - Nav: insert immediately after the nav item added by the previous task (the `href="#collapsible-section"` item), inside the `Form building blocks` group.
  - Sections: insert immediately before the line `<div style="height:48px"></div>` (after the section added by the previous task).

**Interfaces:**

Consumes:
- `SavedPill` from `@/components/ui/saved-pill` (extended in this task with `persistent` and `children`).
- `Input`, `Button`, `Tooltip`, `TooltipContent`, `TooltipProvider`, `TooltipTrigger` from `@/components/ui/*`.
- `Plus`, `X` from `lucide-react`; `cn` from `@/lib/utils`.
- Existing behaviour to preserve from `src/components/settings/AgentConfigsDialog.tsx`: the keychain credential field (password input, "Saved in the keychain" state, a remove-saved-credential toggle that also clears the typed value, lines ~765-801) and the env var rows (name and value inputs, per-row remove button with an `X`, per-row error with `role='alert'`, Add button with `Plus`, rows keyed by a stable `key`, lines ~640-700).

Produces (exact contract):

```ts
// saved-pill.tsx (extended, backwards compatible)
export function SavedPill(props?: { persistent?: boolean; children?: ReactNode }): JSX.Element | null;

// secret-field.tsx
export function SecretField(props: {
  id: string;
  value: string;
  onValueChange: (value: string) => void;
  hasSaved: boolean;
  markedForRemoval: boolean;
  onToggleRemove: () => void;
  placeholder?: string;
  'aria-invalid'?: boolean;
  'aria-describedby'?: string;
}): JSX.Element;

// key-value-rows.tsx
export interface KeyValueRow { key: string; name: string; value: string }
export function KeyValueRows(props: {
  rows: KeyValueRow[];
  onChange: (rows: KeyValueRow[]) => void;
  columns: 'pair' | 'single';
  nameLabel?: string;
  valueLabel: string;
  addLabel: string;
  rowErrors?: (string | null)[];
}): JSX.Element;

// dialog.tsx
export function DialogBody(props: HTMLAttributes<HTMLDivElement>): JSX.Element;
```

SecretField behaviour (preserves today's dialog, replaces the icon-only toggle with text buttons):
- No saved credential: a password `Input` with the placeholder (default `Paste the token`).
- Saved, not marked, not replacing, empty value: a persistent pill `Saved in the keychain` with text buttons `Replace` and `Remove`, and no input.
- `Replace` shows the password input (focused) and a `Cancel` text button that clears the typed value and returns to the saved state.
- `Remove` clears the typed value, calls `onToggleRemove`, and shows `The saved credential will be removed when you save.` with an `Undo` text button that calls `onToggleRemove`.
- A non-empty `value` always shows the input, so a parent-supplied value is never hidden.

- [ ] **Step 1: Write the failing SecretField test (including the SavedPill extension)**

Create `src/components/ui/__tests__/secret-field.test.tsx`:

```tsx
import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { SavedPill } from '@/components/ui/saved-pill';
import { SecretField } from '@/components/ui/secret-field';

function Controlled(props: { hasSaved: boolean; initialRemoved?: boolean; initialValue?: string }) {
  const [value, setValue] = useState(props.initialValue ?? '');
  const [removed, setRemoved] = useState(props.initialRemoved ?? false);
  return (
    <SecretField
      id='cred'
      value={value}
      onValueChange={setValue}
      hasSaved={props.hasSaved}
      markedForRemoval={removed}
      onToggleRemove={() => setRemoved((current) => !current)}
    />
  );
}

describe('SavedPill', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('still hides itself after 3 seconds by default', () => {
    vi.useFakeTimers();
    render(<SavedPill />);
    expect(screen.getByText('All changes saved')).toBeInTheDocument();
    act(() => {
      vi.advanceTimersByTime(3000);
    });
    expect(screen.queryByText('All changes saved')).toBeNull();
  });

  it('stays visible and shows a custom label when persistent', () => {
    vi.useFakeTimers();
    render(<SavedPill persistent>Saved in the keychain</SavedPill>);
    act(() => {
      vi.advanceTimersByTime(10000);
    });
    expect(screen.getByText('Saved in the keychain')).toBeInTheDocument();
  });
});

describe('SecretField', () => {
  it('shows a password input when nothing is saved', () => {
    render(<Controlled hasSaved={false} />);
    const input = screen.getByPlaceholderText('Paste the token');
    expect(input).toHaveAttribute('type', 'password');
    expect(input).toHaveAttribute('id', 'cred');
    expect(screen.queryByRole('button', { name: 'Replace' })).toBeNull();
  });

  it('shows the saved pill with Replace and Remove text buttons', () => {
    render(<Controlled hasSaved />);
    expect(screen.getByText('Saved in the keychain')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Replace' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Remove' })).toBeInTheDocument();
    expect(screen.queryByPlaceholderText('Paste the token')).toBeNull();
  });

  it('shows the input after Replace and returns on Cancel', async () => {
    render(<Controlled hasSaved />);
    await userEvent.click(screen.getByRole('button', { name: 'Replace' }));
    const input = screen.getByPlaceholderText('Paste the token');
    expect(input).toHaveFocus();
    await userEvent.type(input, 'secret');
    expect(input).toHaveValue('secret');
    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(screen.queryByPlaceholderText('Paste the token')).toBeNull();
    expect(screen.getByText('Saved in the keychain')).toBeInTheDocument();
  });

  it('marks the credential for removal and can undo', async () => {
    render(<Controlled hasSaved />);
    await userEvent.click(screen.getByRole('button', { name: 'Remove' }));
    expect(
      screen.getByText('The saved credential will be removed when you save.'),
    ).toBeInTheDocument();
    expect(screen.queryByText('Saved in the keychain')).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: 'Undo' }));
    expect(screen.getByText('Saved in the keychain')).toBeInTheDocument();
  });

  it('keeps the input visible when a value is present', () => {
    render(
      <SecretField
        id='cred'
        value='typed'
        onValueChange={() => {}}
        hasSaved
        markedForRemoval={false}
        onToggleRemove={() => {}}
      />,
    );
    // A non-empty value keeps the input visible, so Replace and Remove are not shown.
    expect(screen.getByDisplayValue('typed')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Remove' })).toBeNull();
  });

  it('calls onValueChange with an empty string and toggles removal on Remove', async () => {
    const onValueChange = vi.fn();
    const onToggleRemove = vi.fn();
    render(
      <SecretField
        id='cred'
        value=''
        onValueChange={onValueChange}
        hasSaved
        markedForRemoval={false}
        onToggleRemove={onToggleRemove}
      />,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Remove' }));
    expect(onValueChange).toHaveBeenCalledWith('');
    expect(onToggleRemove).toHaveBeenCalledTimes(1);
  });

  it('forwards aria props and a custom placeholder to the input', () => {
    render(
      <SecretField
        id='cred'
        value=''
        onValueChange={() => {}}
        hasSaved={false}
        markedForRemoval={false}
        onToggleRemove={() => {}}
        placeholder='Paste a key'
        aria-invalid
        aria-describedby='cred-help cred-error'
      />,
    );
    const input = screen.getByPlaceholderText('Paste a key');
    expect(input).toHaveAttribute('aria-invalid', 'true');
    expect(input).toHaveAttribute('aria-describedby', 'cred-help cred-error');
  });

  it('does not use an icon-only button', () => {
    render(<Controlled hasSaved />);
    for (const button of screen.getAllByRole('button')) {
      expect(button.textContent?.trim()).not.toBe('');
    }
  });
});
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `yarn test src/components/ui/__tests__/secret-field.test.tsx`
Expected: FAIL with `Failed to resolve import "@/components/ui/secret-field"`.

- [ ] **Step 3: Extend SavedPill**

Replace the whole of `src/components/ui/saved-pill.tsx` (lines 1-24) with:

```tsx
import { Check } from 'lucide-react';
import { type ReactNode, useEffect, useState } from 'react';

/**
 * Appears for 3 s then hides itself. Re-mount with a new key to restart the timer.
 * Pass `persistent` to keep it visible, and `children` to change the label.
 * Usage: {savedAt && <SavedPill key={savedAt} />}
 */
export function SavedPill({
  persistent = false,
  children = 'All changes saved',
}: {
  persistent?: boolean;
  children?: ReactNode;
}) {
  const [visible, setVisible] = useState(true);

  useEffect(() => {
    if (persistent) return undefined;
    const id = setTimeout(() => setVisible(false), 3000);
    return () => clearTimeout(id);
  }, [persistent]);

  if (!visible) return null;

  return (
    <span className='flex items-center gap-1 text-xs text-muted-foreground'>
      <Check className='h-3 w-3' />
      {children}
    </span>
  );
}
```

- [ ] **Step 4: Implement SecretField**

Create `src/components/ui/secret-field.tsx`:

```tsx
import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { SavedPill } from '@/components/ui/saved-pill';

export interface SecretFieldProps {
  id: string;
  value: string;
  onValueChange: (value: string) => void;
  hasSaved: boolean;
  markedForRemoval: boolean;
  onToggleRemove: () => void;
  placeholder?: string;
  'aria-invalid'?: boolean;
  'aria-describedby'?: string;
}

/**
 * A secret entry that knows about an already stored credential.
 * Saved state shows a pill with Replace and Remove text buttons.
 */
export function SecretField({
  id,
  value,
  onValueChange,
  hasSaved,
  markedForRemoval,
  onToggleRemove,
  placeholder = 'Paste the token',
  'aria-invalid': ariaInvalid,
  'aria-describedby': ariaDescribedBy,
}: SecretFieldProps) {
  const [replacing, setReplacing] = useState(false);

  useEffect(() => {
    if (replacing) document.getElementById(id)?.focus();
  }, [replacing, id]);

  if (hasSaved && markedForRemoval) {
    return (
      <div id={id} className='flex items-center gap-2'>
        <span className='min-w-0 flex-1 break-words text-xs text-destructive'>
          The saved credential will be removed when you save.
        </span>
        <Button variant='outline' size='sm' className='h-7' onClick={onToggleRemove}>
          Undo
        </Button>
      </div>
    );
  }

  const showInput = !hasSaved || replacing || value !== '';

  if (!showInput) {
    return (
      <div id={id} className='flex items-center gap-2'>
        <div className='min-w-0 flex-1'>
          <SavedPill persistent>Saved in the keychain</SavedPill>
        </div>
        <Button variant='outline' size='sm' className='h-7' onClick={() => setReplacing(true)}>
          Replace
        </Button>
        <Button
          variant='outline'
          size='sm'
          className='h-7'
          onClick={() => {
            onValueChange('');
            onToggleRemove();
          }}
        >
          Remove
        </Button>
      </div>
    );
  }

  return (
    <div className='flex items-center gap-2'>
      <Input
        id={id}
        type='password'
        autoComplete='off'
        value={value}
        onChange={(e) => onValueChange(e.target.value)}
        placeholder={placeholder}
        aria-invalid={ariaInvalid}
        aria-describedby={ariaDescribedBy}
        className='h-8 text-sm'
      />
      {hasSaved && (
        <Button
          variant='outline'
          size='sm'
          className='h-7'
          onClick={() => {
            onValueChange('');
            setReplacing(false);
          }}
        >
          Cancel
        </Button>
      )}
    </div>
  );
}
```

- [ ] **Step 5: Run the SecretField test and confirm it passes**

Run: `yarn test src/components/ui/__tests__/secret-field.test.tsx`
Expected: PASS, 10 tests.

- [ ] **Step 6: Write the failing KeyValueRows test**

Create `src/components/ui/__tests__/key-value-rows.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { type KeyValueRow, KeyValueRows } from '@/components/ui/key-value-rows';

function Pair(props: {
  initial: KeyValueRow[];
  rowErrors?: (string | null)[];
  onChange?: (rows: KeyValueRow[]) => void;
}) {
  const [rows, setRows] = useState(props.initial);
  return (
    <KeyValueRows
      rows={rows}
      onChange={(next) => {
        setRows(next);
        props.onChange?.(next);
      }}
      columns='pair'
      nameLabel='Name'
      valueLabel='Value'
      addLabel='Add variable'
      rowErrors={props.rowErrors}
    />
  );
}

function Single(props: { initial: KeyValueRow[] }) {
  const [rows, setRows] = useState(props.initial);
  return (
    <KeyValueRows
      rows={rows}
      onChange={setRows}
      columns='single'
      valueLabel='Argument'
      addLabel='Add argument'
    />
  );
}

const TWO: KeyValueRow[] = [
  { key: 'k1', name: 'A', value: '1' },
  { key: 'k2', name: 'B', value: '2' },
];

describe('KeyValueRows', () => {
  it('renders a name and a value input per row in pair mode', () => {
    render(<Pair initial={TWO} />);
    expect(screen.getByLabelText('Name 1')).toHaveValue('A');
    expect(screen.getByLabelText('Value 1')).toHaveValue('1');
    expect(screen.getByLabelText('Name 2')).toHaveValue('B');
    expect(screen.getByLabelText('Value 2')).toHaveValue('2');
  });

  it('uses a two-column plus remove grid for pair and one-column plus remove for single', () => {
    const { container, rerender } = render(<Pair initial={TWO} />);
    expect(container.querySelector('[data-slot="kv-row"]')).toHaveClass(
      'grid-cols-[1fr_1fr_32px]',
    );
    rerender(<Single initial={[{ key: 'a', name: '', value: 'run' }]} />);
    expect(container.querySelector('[data-slot="kv-row"]')).toHaveClass('grid-cols-[1fr_32px]');
  });

  it('edits only the value in single mode', async () => {
    render(<Single initial={[{ key: 'a', name: '', value: 'run' }]} />);
    expect(screen.queryByLabelText('Name 1')).toBeNull();
    const input = screen.getByLabelText('Argument 1');
    await userEvent.type(input, 's');
    expect(input).toHaveValue('runs');
  });

  it('reports edits with the stable key untouched', async () => {
    const onChange = vi.fn();
    render(<Pair initial={TWO} onChange={onChange} />);
    await userEvent.type(screen.getByLabelText('Name 1'), 'X');
    expect(onChange).toHaveBeenLastCalledWith([
      { key: 'k1', name: 'AX', value: '1' },
      { key: 'k2', name: 'B', value: '2' },
    ]);
  });

  it('removes a row with an accessible ghost button', async () => {
    const onChange = vi.fn();
    render(<Pair initial={TWO} onChange={onChange} />);
    await userEvent.click(screen.getByRole('button', { name: 'Remove name 1' }));
    expect(onChange).toHaveBeenLastCalledWith([{ key: 'k2', name: 'B', value: '2' }]);
    expect(screen.queryByLabelText('Name 2')).toBeNull();
  });

  it('names the remove button after the value label in single mode', async () => {
    render(<Single initial={[{ key: 'a', name: '', value: 'run' }]} />);
    await userEvent.click(screen.getByRole('button', { name: 'Remove argument 1' }));
    expect(screen.queryByLabelText('Argument 1')).toBeNull();
    expect(screen.getByRole('button', { name: 'Add argument' })).toBeInTheDocument();
  });

  it('adds an empty row with a fresh unique key', async () => {
    const onChange = vi.fn();
    render(<Pair initial={TWO} onChange={onChange} />);
    await userEvent.click(screen.getByRole('button', { name: 'Add variable' }));
    const next = onChange.mock.calls.at(-1)?.[0] as KeyValueRow[];
    expect(next).toHaveLength(3);
    expect(next[2]).toMatchObject({ name: '', value: '' });
    expect(new Set(next.map((r) => r.key)).size).toBe(3);
  });

  it('renders only the Add button for empty rows', async () => {
    const onChange = vi.fn();
    const { container } = render(<Pair initial={[]} onChange={onChange} />);
    expect(container.querySelector('[data-slot="kv-row"]')).toBeNull();
    expect(container.querySelector('[data-slot="kv-header"]')).toBeNull();
    expect(screen.queryByRole('textbox')).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: 'Add variable' }));
    expect(onChange).toHaveBeenCalledTimes(1);
    expect((onChange.mock.calls[0]?.[0] as KeyValueRow[]).length).toBe(1);
  });

  it('shows no error for a blank row unless one is given', () => {
    const blank: KeyValueRow[] = [{ key: 'b1', name: '', value: '' }];
    render(<Pair initial={blank} />);
    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.getByLabelText('Name 1')).toHaveAttribute('aria-invalid', 'false');
    expect(screen.getByLabelText('Value 1')).toHaveAttribute('aria-invalid', 'false');
  });

  it('shows a row error and marks the inputs invalid', () => {
    render(<Pair initial={TWO} rowErrors={[null, 'Duplicate name.']} />);
    expect(screen.getByRole('alert')).toHaveTextContent('Duplicate name.');
    expect(screen.getByLabelText('Name 2')).toHaveAttribute('aria-invalid', 'true');
    expect(screen.getByLabelText('Value 2')).toHaveAttribute('aria-invalid', 'true');
    expect(screen.getByLabelText('Name 1')).toHaveAttribute('aria-invalid', 'false');
  });

  it('gives every row control a unique id', () => {
    const { container } = render(<Pair initial={TWO} rowErrors={['Bad.', 'Bad.']} />);
    const ids = Array.from(container.querySelectorAll('[id]')).map((node) => node.id);
    expect(ids.length).toBeGreaterThanOrEqual(6);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it('keeps an add button that is left aligned', () => {
    render(<Pair initial={TWO} />);
    const add = screen.getByRole('button', { name: 'Add variable' });
    expect(add.parentElement).toHaveClass('flex', 'justify-start');
  });
});
```

- [ ] **Step 7: Run it and confirm it fails**

Run: `yarn test src/components/ui/__tests__/key-value-rows.test.tsx`
Expected: FAIL with `Failed to resolve import "@/components/ui/key-value-rows"`.

- [ ] **Step 8: Implement KeyValueRows**

Create `src/components/ui/key-value-rows.tsx`:

```tsx
import { Plus, X } from 'lucide-react';
import { useId } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';

/** `key` is a stable React key and is never edited. */
export interface KeyValueRow {
  key: string;
  name: string;
  value: string;
}

export interface KeyValueRowsProps {
  rows: KeyValueRow[];
  onChange: (rows: KeyValueRow[]) => void;
  columns: 'pair' | 'single';
  nameLabel?: string;
  valueLabel: string;
  addLabel: string;
  rowErrors?: (string | null)[];
}

/**
 * Editable rows. Pair mode edits name and value, single mode edits only the value.
 * It never validates: a row shows an error only when `rowErrors` gives one for its index.
 * Accessible names: inputs "<label> <n>", remove "Remove <label lowercase> <n>", add = addLabel.
 */
export function KeyValueRows({
  rows,
  onChange,
  columns,
  nameLabel = 'Name',
  valueLabel,
  addLabel,
  rowErrors,
}: KeyValueRowsProps) {
  const baseId = useId();
  const isPair = columns === 'pair';
  const gridClass = isPair ? 'grid-cols-[1fr_1fr_32px]' : 'grid-cols-[1fr_32px]';
  const removeNoun = (isPair ? nameLabel : valueLabel).toLowerCase();

  const update = (key: string, patch: Partial<KeyValueRow>) =>
    onChange(rows.map((row) => (row.key === key ? { ...row, ...patch } : row)));

  const remove = (key: string) => onChange(rows.filter((row) => row.key !== key));

  const add = () => onChange([...rows, { key: crypto.randomUUID(), name: '', value: '' }]);

  return (
    <TooltipProvider>
      <div className='space-y-2'>
        {rows.length > 0 && (
          <div
            data-slot='kv-header'
            className={cn('grid gap-2 text-xs text-muted-foreground', gridClass)}
          >
            {isPair && <span>{nameLabel}</span>}
            <span>{valueLabel}</span>
            <span aria-hidden='true' />
          </div>
        )}
        {rows.map((row, index) => {
          const error = rowErrors?.[index] ?? null;
          const errorId = `${baseId}-${row.key}-error`;
          const describedBy = error ? errorId : undefined;
          return (
            <div key={row.key} className='space-y-1'>
              <div data-slot='kv-row' className={cn('grid items-center gap-2', gridClass)}>
                {isPair && (
                  <Input
                    id={`${baseId}-${row.key}-name`}
                    aria-label={`${nameLabel} ${index + 1}`}
                    aria-invalid={error !== null}
                    aria-describedby={describedBy}
                    value={row.name}
                    onChange={(e) => update(row.key, { name: e.target.value })}
                    className='h-8 text-sm'
                  />
                )}
                <Input
                  id={`${baseId}-${row.key}-value`}
                  aria-label={`${valueLabel} ${index + 1}`}
                  aria-invalid={error !== null}
                  aria-describedby={describedBy}
                  value={row.value}
                  onChange={(e) => update(row.key, { value: e.target.value })}
                  className='h-8 text-sm'
                />
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      variant='ghost'
                      size='icon'
                      className='size-8'
                      aria-label={`Remove ${removeNoun} ${index + 1}`}
                      onClick={() => remove(row.key)}
                    >
                      <X className='h-3.5 w-3.5' aria-hidden='true' />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>Remove</TooltipContent>
                </Tooltip>
              </div>
              {error && (
                <p id={errorId} role='alert' className='break-words text-xs text-destructive'>
                  {error}
                </p>
              )}
            </div>
          );
        })}
        <div className='flex justify-start'>
          <Button variant='outline' size='sm' onClick={add}>
            <Plus className='mr-1.5 h-3.5 w-3.5' aria-hidden='true' />
            {addLabel}
          </Button>
        </div>
      </div>
    </TooltipProvider>
  );
}
```

- [ ] **Step 9: Run the KeyValueRows test and confirm it passes**

Run: `yarn test src/components/ui/__tests__/key-value-rows.test.tsx`
Expected: PASS, 12 tests.

- [ ] **Step 10: Write the failing DialogBody test**

Create `src/components/ui/__tests__/dialog-body.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
} from '@/components/ui/dialog';

describe('DialogBody', () => {
  it('is a scrolling flex child that can shrink', () => {
    render(<DialogBody data-testid='body'>content</DialogBody>);
    const body = screen.getByTestId('body');
    expect(body).toHaveAttribute('data-slot', 'dialog-body');
    expect(body).toHaveClass('overflow-auto', 'flex-1', 'min-h-0');
  });

  it('merges a custom className and forwards props', () => {
    render(
      <DialogBody data-testid='body' className='space-y-4' id='b1'>
        content
      </DialogBody>,
    );
    const body = screen.getByTestId('body');
    expect(body).toHaveClass('space-y-4', 'overflow-auto');
    expect(body).toHaveAttribute('id', 'b1');
  });

  it('sits inside a dialog that keeps max-w-lg and a footer', () => {
    render(
      <Dialog open>
        <DialogContent className='flex max-h-[85vh] flex-col'>
          <DialogTitle>Title</DialogTitle>
          <DialogDescription>Description</DialogDescription>
          <DialogBody data-testid='body'>content</DialogBody>
          <DialogFooter data-testid='footer'>actions</DialogFooter>
        </DialogContent>
      </Dialog>,
    );
    expect(screen.getByRole('dialog')).toHaveClass('max-w-lg', 'flex', 'flex-col');
    expect(screen.getByTestId('body')).toBeInTheDocument();
    expect(screen.getByTestId('footer')).toHaveClass('sm:justify-end');
  });
});
```

- [ ] **Step 11: Run it and confirm it fails**

Run: `yarn test src/components/ui/__tests__/dialog-body.test.tsx`
Expected: FAIL (`DialogBody` is not exported from `@/components/ui/dialog`, so rendering throws).

- [ ] **Step 12: Implement DialogBody**

In `src/components/ui/dialog.tsx`, insert this function directly after `DialogFooter` (after the closing brace at line 73, before `DialogTitle`):

```tsx
/** Scrolling region between header and footer. Give DialogContent flex flex-col and a max height. */
function DialogBody({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      data-slot='dialog-body'
      className={cn('min-h-0 flex-1 overflow-auto', className)}
      {...props}
    />
  );
}
```

Then add `DialogBody,` to the export list, in alphabetical order, between `DialogClose,` and `DialogContent,`:

```tsx
export {
  Dialog,
  DialogBody,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogOverlay,
  DialogPortal,
  DialogTitle,
  DialogTrigger,
};
```

- [ ] **Step 13: Run all three tests and confirm they pass**

Run: `yarn test src/components/ui/__tests__/secret-field.test.tsx src/components/ui/__tests__/key-value-rows.test.tsx src/components/ui/__tests__/dialog-body.test.tsx`
Expected: PASS, 25 tests.

- [ ] **Step 14: Add the DSM entries (CSS, nav, sections)**

In `docs/dsm/index.html`, with the Edit tool:

(a) CSS. Replace the line `/* ─── overlays (DSM sections) ─── */` with the block below followed by that same line:

```css
/* ── Secret field (ui/secret-field.tsx) ── */
.c-sf { display: flex; align-items: center; gap: 8px; width: 360px; max-width: 100%; }
.c-sf-pill { min-width: 0; flex: 1; }
.c-sf-remove { min-width: 0; flex: 1; font-size: 12px; color: var(--destructive); overflow-wrap: break-word; }

/* ── Key and value rows (ui/key-value-rows.tsx) ── */
.c-kv { display: flex; flex-direction: column; gap: 8px; width: 360px; max-width: 100%; }
.c-kv-row { display: grid; grid-template-columns: 1fr 1fr 32px; align-items: center; gap: 8px; }
.c-kv-row.is-single { grid-template-columns: 1fr 32px; }
.c-kv-head { display: grid; grid-template-columns: 1fr 1fr 32px; gap: 8px; font-size: 12px; color: var(--muted-foreground); }
.c-kv-head.is-single { grid-template-columns: 1fr 32px; }
.c-kv .c-input { height: 32px; font-size: 14px; }
.c-kv-x { display: inline-flex; align-items: center; justify-content: center; width: 32px; height: 32px; border-radius: var(--radius-md); color: var(--foreground); }
.c-kv-x svg { width: 14px; height: 14px; }
.c-kv-x:hover { background: var(--accent); color: var(--accent-foreground); }
.c-kv-err { margin: 0; font-size: 12px; color: var(--destructive); overflow-wrap: break-word; }

/* ── Dialog body (ui/dialog.tsx) ── */
.c-db { display: flex; flex-direction: column; width: 360px; max-width: 100%; max-height: 200px; box-sizing: border-box; padding: 16px; gap: 12px; border: 1px solid var(--border); border-radius: var(--radius-lg, 8px); background: var(--card); }
.c-db-body { min-height: 0; flex: 1; overflow: auto; font-size: 12px; color: var(--muted-foreground); }
.c-db-foot { display: flex; justify-content: flex-end; gap: 8px; }
```

(b) Nav. Replace the `href="#collapsible-section"` nav line (added by Task 2) with itself followed by these three lines:

```html
      <a class="nav-item" href="#secret-field" onclick="scrollTo('secret-field',this)"><span class="dot"></span> Secret field</a>
      <a class="nav-item" href="#key-value-rows" onclick="scrollTo('key-value-rows',this)"><span class="dot"></span> Key and value rows</a>
      <a class="nav-item" href="#dialog-body" onclick="scrollTo('dialog-body',this)"><span class="dot"></span> Dialog body</a>
```

(c) Sections. Replace the line `<div style="height:48px"></div>` with the three sections below followed by that same line:

```html
<section class="dsm-section" id="secret-field">
        <div class="section-title">41 — Form building blocks</div>
        <div class="section-heading">Secret field</div>
        <p class="section-desc">A password input that knows about an already stored credential. The saved state reuses the Saved pill (persistent mode) and offers Replace and Remove as text buttons, never an icon-only toggle. Removal is a pending state that Undo reverses.</p>
<div class="dsm-card"><div class="component-label">Nothing saved, saved, and marked for removal</div><div class="c-stack"><div class="c-sf"><input class="c-input" style="height:32px;font-size:14px" type="password" placeholder="Paste the token"></div><div class="c-sf"><div class="c-sf-pill"><span class="c-pill"><svg class="c-ic-12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>Saved in the keychain</span></div><span class="c-btn c-btn-outline c-btn-sm" style="height:28px">Replace</span><span class="c-btn c-btn-outline c-btn-sm" style="height:28px">Remove</span></div><div class="c-sf"><span class="c-sf-remove">The saved credential will be removed when you save.</span><span class="c-btn c-btn-outline c-btn-sm" style="height:28px">Undo</span></div></div><div class="component-label" style="margin-top:16px">Spec</div><div class="c-spec-wrap"><table class="c-spec"><thead><tr><th>Part</th><th>Tailwind classes</th><th>Source</th></tr></thead><tbody><tr><td>Input</td><td>Input type=password autoComplete=off h-8 text-sm; forwards id, aria-invalid, aria-describedby</td><td>secret-field.tsx:94</td></tr><tr><td>Saved pill</td><td>SavedPill persistent: flex items-center gap-1 text-xs text-muted-foreground</td><td>secret-field.tsx:62, saved-pill.tsx:16-33</td></tr><tr><td>Actions</td><td>Button outline sm h-7 with text labels Replace, Remove, Cancel, Undo</td><td>secret-field.tsx:64-80</td></tr><tr><td>Removal note</td><td>min-w-0 flex-1 break-words text-xs text-destructive</td><td>secret-field.tsx:47</td></tr><tr><td>Rules</td><td>A non-empty value always shows the input. Remove clears the typed value and calls onToggleRemove. Replace focuses the input.</td><td>secret-field.tsx:57</td></tr></tbody></table></div></div>
      </section>

<section class="dsm-section" id="key-value-rows">
        <div class="section-title">42 — Form building blocks</div>
        <div class="section-heading">Key and value rows</div>
        <p class="section-desc">Editable rows for extra environment variables (pair mode) and arguments (single mode, value only). The grid is <code class="o-code">1fr 1fr 32px</code> for a pair and <code class="o-code">1fr 32px</code> for a single column. Remove is a ghost icon Button with an aria-label and a Tooltip. Add sits below, left aligned. Rows carry a stable key, so editing never remounts an input.</p>
<div class="dsm-card"><div class="component-label">Pair with an error, and single</div><div class="c-stack"><div class="c-kv"><div class="c-kv-head"><span>Name</span><span>Value</span><span></span></div><div class="c-kv-row"><input class="c-input" type="text" value="HTTP_PROXY" aria-label="Name 1"><input class="c-input" type="text" value="http://proxy" aria-label="Value 1"><span class="c-kv-x"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span></div><div class="c-kv-row"><input class="c-input is-invalid" type="text" value="bad name" aria-label="Name 2" aria-invalid="true"><input class="c-input is-invalid" type="text" value="1" aria-label="Value 2" aria-invalid="true"><span class="c-kv-x"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span></div><p class="c-kv-err" role="alert">Use letters, digits and underscores only.</p><div><span class="c-btn c-btn-outline c-btn-sm">Add variable</span></div></div><div class="c-kv"><div class="c-kv-row is-single"><input class="c-input" type="text" value="--experimental-acp" aria-label="Argument 1"><span class="c-kv-x"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></span></div><div><span class="c-btn c-btn-outline c-btn-sm">Add argument</span></div></div></div><div class="component-label" style="margin-top:16px">Spec</div><div class="c-spec-wrap"><table class="c-spec"><thead><tr><th>Part</th><th>Tailwind classes</th><th>Source</th></tr></thead><tbody><tr><td>Row grid</td><td>grid items-center gap-2; pair grid-cols-[1fr_1fr_32px], single grid-cols-[1fr_32px]</td><td>key-value-rows.tsx:65</td></tr><tr><td>Header</td><td>grid gap-2 text-xs text-muted-foreground; hidden for zero rows</td><td>key-value-rows.tsx:54</td></tr><tr><td>Inputs</td><td>Input h-8 text-sm, aria-label "{label} {n}", aria-invalid on error</td><td>key-value-rows.tsx:67-85</td></tr><tr><td>Remove</td><td>Button ghost icon size-8, X h-3.5 w-3.5, aria-label "Remove {label} {n}", Tooltip "Remove"</td><td>key-value-rows.tsx:87-98</td></tr><tr><td>Row error</td><td>role="alert" break-words text-xs text-destructive</td><td>key-value-rows.tsx:101</td></tr><tr><td>Add</td><td>Button outline sm with Plus h-3.5 w-3.5, in a flex justify-start wrapper; its text is addLabel</td><td>key-value-rows.tsx:107</td></tr><tr><td>Validation</td><td>None here. A row shows an error only when rowErrors has one for its index, so a blank row shows none. The parent owns the rules.</td><td>key-value-rows.tsx:40</td></tr></tbody></table></div></div>
      </section>

<section class="dsm-section" id="dialog-body">
        <div class="section-title">43 — Form building blocks</div>
        <div class="section-heading">Dialog body</div>
        <p class="section-desc">A scrolling region for long forms inside a dialog. <code class="o-code">DialogContent</code> keeps <code class="o-code">max-w-lg</code>. For a long form pass <code class="o-code">className="flex max-h-[85vh] flex-col"</code> to DialogContent, so the header and footer stay put while the body scrolls. The footer stays right aligned.</p>
<div class="dsm-card"><div class="component-label">Header and footer fixed, body scrolls</div><div class="c-db"><div class="c-cs-title">Edit agent</div><div class="c-db-body"><p>Long form content. The body scrolls when it is taller than the dialog.</p><p>Agent section.</p><p>Credential section.</p><p>Advanced section.</p><p>More content.</p><p>More content.</p></div><div class="c-db-foot"><span class="c-btn c-btn-outline">Cancel</span><span class="c-btn c-btn-default">Save</span></div></div><div class="component-label" style="margin-top:16px">Spec</div><div class="c-spec-wrap"><table class="c-spec"><thead><tr><th>Part</th><th>Tailwind classes</th><th>Source</th></tr></thead><tbody><tr><td>Body</td><td>min-h-0 flex-1 overflow-auto (data-slot="dialog-body")</td><td>dialog.tsx:75-83</td></tr><tr><td>Content (caller)</td><td>keeps max-w-lg; add flex max-h-[85vh] flex-col for a scrolling dialog</td><td>dialog.tsx:38</td></tr><tr><td>Footer</td><td>flex flex-col-reverse sm:flex-row sm:justify-end sm:space-x-2 (unchanged)</td><td>dialog.tsx:67</td></tr></tbody></table></div></div>
      </section>

<div style="height:48px"></div>
```

Also update the existing Saved pill spec row in `docs/dsm/index.html` (section `saved-pill`, about line 2074): replace `<tr><td>Behaviour</td><td>Visible 3 s then unmounts; remount with a new key to restart</td><td>saved-pill.tsx:11-15</td></tr>` with `<tr><td>Behaviour</td><td>Visible 3 s then unmounts; remount with a new key to restart. The persistent prop keeps it visible and children replace the label.</td><td>saved-pill.tsx:15-23</td></tr>`.

Note: the line numbers in the spec tables are approximate. After the implementation files are final, adjust them to the real line numbers.

- [ ] **Step 15: Run the type check, the linter and the full ui test folder**

Run: `yarn tsc --noEmit`
Expected: exits 0, no errors.

Run: `yarn check`
Expected: exits 0. If Biome reports formatting in the new files, run `yarn format` and re-run `yarn check`.

Run: `yarn test src/components/ui`
Expected: PASS for all tests under `src/components/ui`, including the existing `number-input.test.tsx`.

- [ ] **Step 16: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill and commit with a pathspec. Suggested message: `feat(ui): add SecretField, KeyValueRows and DialogBody with DSM entries`.

```bash
git add src/components/ui/secret-field.tsx src/components/ui/key-value-rows.tsx \
  src/components/ui/saved-pill.tsx src/components/ui/dialog.tsx \
  src/components/ui/__tests__/secret-field.test.tsx src/components/ui/__tests__/key-value-rows.test.tsx \
  src/components/ui/__tests__/dialog-body.test.tsx docs/dsm/index.html
git commit --only -m "feat(ui): add SecretField, KeyValueRows and DialogBody with DSM entries" -- \
  src/components/ui/secret-field.tsx src/components/ui/key-value-rows.tsx \
  src/components/ui/saved-pill.tsx src/components/ui/dialog.tsx \
  src/components/ui/__tests__/secret-field.test.tsx src/components/ui/__tests__/key-value-rows.test.tsx \
  src/components/ui/__tests__/dialog-body.test.tsx docs/dsm/index.html
```
