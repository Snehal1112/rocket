# Consumer Tree Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the collapsed "+N consumers" badge in `ContractCard` with a vertical tree that shows each consumer as its own `PartyPill` node, capped at 5 visible pills with a "+N more" overflow node.

**Architecture:** A new focused component `ConsumerTree` encapsulates the branching layout using pure CSS (border lines, no SVG). `ContractCard` replaces its multi-consumer branch with `<ConsumerTree>`, while the single-consumer path stays untouched. `PartyPill` and all other card internals are unchanged.

**Tech Stack:** React, TypeScript, Tailwind CSS, shadcn/ui `Tooltip`/`TooltipProvider`/`TooltipContent`/`TooltipTrigger`

---

## File Map

| Action | File | Responsibility |
|--------|------|----------------|
| Create | `src/components/contracts/ConsumerTree.tsx` | Vertical tree of consumer pills with overflow cap |
| Modify | `src/components/contracts/ContractCard.tsx` | Replace multi-consumer branch with `<ConsumerTree>` |

---

### Task 1: Create `ConsumerTree` component

**Files:**
- Create: `src/components/contracts/ConsumerTree.tsx`

- [ ] **Step 1: Create the file with this exact content**

```tsx
import { TooltipProvider, Tooltip, TooltipTrigger, TooltipContent } from '@/components/ui/tooltip';
import type { Party } from '@/types/contracts';
import { PartyPill } from './PartyPill';

const MAX_VISIBLE = 5;

interface ConsumerTreeProps {
  consumers: Party[];
  /** Override the default visible cap of 5. */
  maxVisible?: number;
}

/**
 * Renders a vertical branching tree of consumer PartyPills.
 * Caps visible pills at `maxVisible` (default 5); any remainder
 * is shown as a "+N more" overflow node with a tooltip listing names.
 */
export function ConsumerTree({ consumers, maxVisible = MAX_VISIBLE }: ConsumerTreeProps) {
  const visible = consumers.slice(0, maxVisible);
  const overflow = consumers.slice(maxVisible);
  const hasOverflow = overflow.length > 0;

  const rows = hasOverflow ? [...visible, null] : visible;

  return (
    <div className='flex flex-col gap-1.5 relative pl-3'>
      {/* Vertical connector line running along the left edge */}
      <span
        aria-hidden='true'
        className='absolute left-0 top-[10px] bottom-[10px] w-px bg-border'
      />

      {rows.map((consumer, i) => {
        const isLast = i === rows.length - 1;

        return (
          <div key={consumer ? consumer.id : '__overflow'} className='relative flex items-center'>
            {/* Horizontal tick from vertical line to pill */}
            <span
              aria-hidden='true'
              className='absolute -left-3 top-1/2 -translate-y-1/2 w-3 h-px bg-border'
            />

            {consumer === null ? (
              /* Overflow node */
              <TooltipProvider delayDuration={200}>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <div className='inline-flex items-center gap-[7px] pl-[8px] pr-[10px] py-1 border border-border rounded-full bg-card text-xs text-muted-foreground cursor-default select-none'>
                      +{overflow.length} more
                    </div>
                  </TooltipTrigger>
                  <TooltipContent side='right'>
                    {overflow.map((c) => c.name).join(', ')}
                  </TooltipContent>
                </Tooltip>
              </TooltipProvider>
            ) : (
              <PartyPill party={consumer} partyRole='consumer' />
            )}
          </div>
        );
      })}
    </div>
  );
}
```

- [ ] **Step 2: Type-check the new file**

```bash
cd /home/numericlabs/data/rocket/rocket && yarn tsc --noEmit 2>&1 | head -40
```

Expected: no errors referencing `ConsumerTree.tsx`.

- [ ] **Step 3: Commit the new component**

```bash
git add src/components/contracts/ConsumerTree.tsx
git commit -m "feat(contracts): add ConsumerTree component for multi-consumer display"
```

---

### Task 2: Wire `ConsumerTree` into `ContractCard`

**Files:**
- Modify: `src/components/contracts/ContractCard.tsx` (lines 261–287)

- [ ] **Step 1: Add the import for `ConsumerTree`**

In `ContractCard.tsx`, find the existing import block near the top (around line 27) that imports from `./PartyPill`:

```tsx
import { PartyPill } from './PartyPill';
```

Add `ConsumerTree` import directly below it:

```tsx
import { ConsumerTree } from './ConsumerTree';
```

- [ ] **Step 2: Replace the multi-consumer branch**

Locate the "Parties" section (around line 258–288). It currently reads:

```tsx
{/* Parties */}
<div className='flex items-center gap-2 flex-wrap mb-3'>
  <PartyPill party={contract.provider} partyRole='provider' />
  <ArrowRight className='w-4 h-3.5 text-muted-foreground shrink-0' aria-hidden='true' />
  {contract.consumers.length === 1 ? (
    <PartyPill party={contract.consumers[0]} partyRole='consumer' />
  ) : (
    <>
      <TooltipProvider delayDuration={300}>
        <Tooltip>
          <TooltipTrigger asChild>
            <div className='inline-flex items-center gap-[7px] pl-[4px] pr-[10px] py-1 border border-border rounded-full bg-card text-xs text-foreground cursor-default'>
              <span className='w-5 h-5 rounded-full bg-primary/20 text-primary text-[10px] font-semibold flex items-center justify-center shrink-0'>
                +{contract.consumers.length}
              </span>
              <span>{contract.consumers.length} consumers</span>
              <span className='text-[10px] text-muted-foreground font-medium shrink-0'>
                · Consumer
              </span>
            </div>
          </TooltipTrigger>
          <TooltipContent side='top'>
            {contract.consumers.map((c) => c.name).join(', ')}
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>
      <span className='text-xs text-muted-foreground truncate max-w-[200px]'>
        · {contract.consumers.map((c) => c.name).join(', ')}
      </span>
    </>
  )}
</div>
```

Replace it with:

```tsx
{/* Parties */}
<div className='flex items-start gap-2 flex-wrap mb-3'>
  <PartyPill party={contract.provider} partyRole='provider' />
  <ArrowRight className='w-4 h-3.5 text-muted-foreground shrink-0 mt-[7px]' aria-hidden='true' />
  {contract.consumers.length === 1 ? (
    <PartyPill party={contract.consumers[0]} partyRole='consumer' />
  ) : (
    <ConsumerTree consumers={contract.consumers} />
  )}
</div>
```

Note: `items-start` (was `items-center`) keeps the provider pill and arrow pinned to the top when the tree grows tall. The `mt-[7px]` on `ArrowRight` nudges it to align with the first consumer pill's vertical center.

- [ ] **Step 3: Remove now-unused imports**

After the replacement, `Tooltip`, `TooltipContent`, `TooltipProvider`, and `TooltipTrigger` are no longer imported directly in `ContractCard.tsx`. Remove them from the import line at the top:

Before:
```tsx
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
```

After: delete that line entirely (the imports are now inside `ConsumerTree.tsx`).

- [ ] **Step 4: Run type-check and lint**

```bash
cd /home/numericlabs/data/rocket/rocket && yarn tsc --noEmit 2>&1 | head -40
```

Expected: no errors.

```bash
cd /home/numericlabs/data/rocket/rocket && yarn check 2>&1 | tail -20
```

Expected: no errors or only pre-existing warnings unrelated to these files.

- [ ] **Step 5: Commit the wiring**

```bash
git add src/components/contracts/ContractCard.tsx
git commit -m "feat(contracts): wire ConsumerTree into ContractCard for multi-consumer layout"
```

---

## Self-Review

**Spec coverage:**
- [x] Vertical tree layout with one node per consumer — Task 1 (ConsumerTree)
- [x] CSS connector lines, no SVG — Task 1 (border-based `span` elements)
- [x] Cap at 5, "+N more" overflow node with tooltip — Task 1 (`overflow` slice + overflow node)
- [x] Single-consumer path unchanged — Task 2 (branch preserved as-is)
- [x] `PartyPill` reused as-is — Task 1 (imported directly)
- [x] `ContractCard` multi-consumer branch replaced — Task 2
- [x] Unused tooltip imports cleaned up — Task 2, Step 3
- [x] Type-check gate after each task — Tasks 1 and 2

**Placeholder scan:** None found.

**Type consistency:** `Party` type used consistently from `@/types/contracts`. `ConsumerTreeProps` interface matches usage in `ContractCard`. `PartyPill` props (`party`, `partyRole`) match its existing interface.
