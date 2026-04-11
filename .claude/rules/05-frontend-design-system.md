# Frontend design system rules

Rules for implementing UI that stays consistent with the existing Rocket design system. **Do not change the design system itself** — no new CSS variables, no new Tailwind theme keys, no new radius scale, no new fonts. If a design spec asks for something the system cannot express, stop and ask the user.

## Stack (what's actually installed)

- **Tailwind** — configured in `tailwind.config.js`. `darkMode: ['class']` — the `.dark` class on an ancestor toggles dark variables.
- **shadcn/ui** — `new-york` style, `neutral` base, CSS variables, `lucide` icons. Config in `components.json`. Do not `npx shadcn add` without user consent — adds files that need review.
- **Fonts** — Inter Variable (body) and JetBrains Mono (code). Already imported in `src/globals.css`. Do not import additional fonts.
- **Icons** — Lucide only. Standardized globally to `stroke-width: 1.5` in `src/globals.css` — do not override per-icon unless a design spec requires it.
- **Animation** — `tailwindcss-animate` and `tw-animate-css` plugins. Use their utilities (`animate-in`, `fade-in`, etc.) rather than writing keyframes.

## Theme tokens (the only colors you may use)

All colors live in `src/globals.css` as HSL CSS variables, consumed via Tailwind semantic classes or `hsl(var(--token))`. The mapping is declared in `tailwind.config.js`.

| Token | Tailwind class | Use for |
|---|---|---|
| `--background` / `--foreground` | `bg-background` / `text-foreground` | Page and text |
| `--card` / `--card-foreground` | `bg-card` / `text-card-foreground` | Cards, panels, elevated surfaces |
| `--popover` / `--popover-foreground` | `bg-popover` / `text-popover-foreground` | Dropdowns, popovers, tooltips |
| `--primary` / `--primary-foreground` | `bg-primary` / `text-primary-foreground` | Primary buttons, active states, links |
| `--secondary` / `--secondary-foreground` | `bg-secondary` / `text-secondary-foreground` | Secondary buttons, subtle backgrounds |
| `--muted` / `--muted-foreground` | `bg-muted` / `text-muted-foreground` | Disabled, placeholder, secondary text |
| `--accent` / `--accent-foreground` | `bg-accent` / `text-accent-foreground` | Hover states on ghost/outline buttons |
| `--destructive` / `--destructive-foreground` | `bg-destructive` / `text-destructive-foreground` | Delete actions, errors |
| `--warning` / `--warning-foreground` | `bg-warning` / `text-warning-foreground` | Warnings, "expiring soon" states |
| `--border` / `--input` / `--ring` | `border-border` / `border-input` / `ring-ring` | Borders, inputs, focus rings |
| `--chart-1..5` | `text-chart-1`..`text-chart-5` | Chart series only |

**Hard rules:**
- Never hardcode a hex or rgb value. If you think you need one, you're fighting the system — use an existing token or ask the user.
- Never introduce a new CSS variable. If the design asks for a color not in the table, stop and flag it.
- Never use Tailwind's default color palette (`text-blue-500`, `bg-slate-200`, etc.). Use semantic tokens only.
- Status colors that need nuance use **opacity modifiers** on an existing token: `bg-primary/10`, `text-destructive/80`. Do not create new tokens.

## Radius scale

Defined in `globals.css` as `--radius: 0.7rem`, extended in `tailwind.config.js`:

| Token | Tailwind class | Size |
|---|---|---|
| `--radius` | `rounded-lg` | `0.7rem` |
| derived | `rounded-md` | `calc(--radius - 2px)` |
| derived | `rounded-sm` | `calc(--radius - 4px)` |

Use `rounded-sm` / `rounded-md` / `rounded-lg`. Never `rounded-xl`, `rounded-2xl`, or arbitrary `rounded-[12px]`.

## Typography

- Body text: default (Inter). No font family class needed.
- Code / mono: `font-mono` (JetBrains Mono).
- Sizes: prefer the scale `text-xs` (11 px), `text-sm` (13 px), `text-base` (14 px). `text-xs` is the default for dense UI surfaces in this project; match the surrounding component.
- Weight: `font-medium` for labels, `font-semibold` for headings and emphasis, `font-normal` otherwise. Do not use `font-bold` in body content — it's too heavy for Inter at small sizes.
- No `tracking-*` overrides unless the design spec explicitly calls for it.

## Available shadcn primitives

These are in `src/components/ui/` and ready to use:

`alert-dialog`, `badge`, `button`, `card`, `checkbox`, `collapsible`, `context-menu`, `dialog`, `dropdown-menu`, `input`, `label`, `popover`, `radio-group`, `resizable`, `saved-pill`, `scroll-area`, `select`, `separator`, `table`, `tabs`, `textarea`, `tooltip`, `tree`.

**Not installed** (do not assume they exist):
- `sheet` — side panels use `dialog` with wider `max-w-*` as a fallback (see `ContractPanel.tsx`)
- `toast` / `sonner` — no notification system yet, error feedback currently goes to `console.error` (tracked in `.claude/sidebar-known-issues.md`)
- `form` / `react-hook-form` integration — forms wire `useState` + `onChange` handlers directly (see `AttachContractDialog.tsx`)
- `combobox` — use `select` or build a `popover` + `input` combination
- `calendar` / `date-picker` — use `<Input type='date'>`

**If you need one of the missing primitives, stop and ask.** Do not `npx shadcn add` without user consent — it touches `components.json`, installs new deps, and the generated file needs review against this project's conventions.

## Variant pattern (building new components)

Follow the CVA pattern used throughout `src/components/ui/`:

```tsx
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '@/lib/utils';

const myVariants = cva(
  'base-classes-here', // layout, shape, default color
  {
    variants: {
      variant: {
        default: 'bg-primary text-primary-foreground',
        muted: 'bg-muted text-muted-foreground',
      },
      size: {
        sm: 'h-7 px-2 text-xs',
        default: 'h-8 px-3 text-sm',
      },
    },
    defaultVariants: { variant: 'default', size: 'default' },
  },
);

type MyProps = React.ComponentProps<'div'> & VariantProps<typeof myVariants>;

export function My({ className, variant, size, ...props }: MyProps) {
  return <div className={cn(myVariants({ variant, size }), className)} {...props} />;
}
```

Rules:
- `cn()` (from `@/lib/utils`) merges classes — use it instead of template literals for conditional classes.
- `className` is the **last** argument so caller overrides win.
- Always accept `className` as a prop so callers can tweak spacing / layout.
- `data-slot='<name>'` on root elements when composing from primitives — matches the shadcn convention for CSS targeting.

## Tailwind class ordering

Order classes in this sequence. Consistency helps diffs and reviews:

1. **Layout** — `flex`, `grid`, `block`, `inline-flex`
2. **Position** — `relative`, `absolute`, `top-*`, `z-*`
3. **Sizing** — `w-*`, `h-*`, `size-*`, `min-w-*`, `max-w-*`
4. **Spacing** — `p-*`, `px-*`, `py-*`, `m-*`, `gap-*`
5. **Typography** — `text-*`, `font-*`, `leading-*`, `truncate`
6. **Color** — `bg-*`, `text-*`, `border-*`, `ring-*`
7. **Shape** — `rounded-*`, `border`, `shadow-*`
8. **State** — `hover:*`, `focus:*`, `disabled:*`, `aria-*:*`, `data-*:*`
9. **Responsive** — `sm:*`, `md:*`, `lg:*`
10. **Dark mode** — `dark:*`

Biome enforces most of this automatically via `yarn format`. If Biome re-orders your classes, accept it — do not fight the formatter.

## Composition rules

- **Primitives first.** If a shadcn primitive covers 80% of what you need, use it and compose. Do not rebuild a Dialog from a `<div>`.
- **Do not edit `src/components/ui/`.** Those files are the design system. Feature-specific variants go in the feature folder and accept props from the primitive.
- **Feature components live in domain folders** — `src/components/contract/`, `src/components/collections/`, etc. Never in `src/components/ui/`.
- **Composition over flags.** Prefer a new component that composes primitives over adding a tenth variant to an existing one.

## Spacing scale

Use Tailwind's 4px-based scale. In practice:

- `gap-1` / `p-1` (4 px) — tightest, between icon and adjacent text
- `gap-2` / `p-2` (8 px) — standard inline spacing
- `gap-3` / `p-3` (12 px) — card padding, form field vertical rhythm
- `gap-4` / `p-4` (16 px) — section padding
- `gap-6` / `p-6` (24 px) — dialog content padding

Do not use arbitrary spacing (`p-[13px]`, `gap-[7px]`). If you need a custom value, match to the closest scale step.

## Icons

- Lucide only. Import from `lucide-react`: `import { Lock, FileText } from 'lucide-react'`.
- Default size: `size-4` (16 px) inline with text, `size-5` (20 px) for toolbar icons, `size-6` (24 px) for feature icons.
- Never override `stroke-width` at the component level — the global rule in `globals.css` is the source of truth.
- Never use emoji as icons. Ever.

## Dark mode

- The `.dark` class toggles the dark variable set in `globals.css`. You do not need to write `dark:` classes for colors — semantic tokens already switch automatically.
- Only use `dark:` variants for things the token system cannot express (e.g., a specific box-shadow or a backdrop blur that needs tuning per mode). Audit carefully before adding them.
- Never hardcode a dark-mode color with `dark:bg-[#...]`.

## Forms

- `<Input>`, `<Textarea>`, `<Select>`, `<Checkbox>`, `<RadioGroup>`, `<Label>` are the building blocks. Never use raw `<input>` except inside a shadcn primitive.
- Labels go above inputs, with `space-y-1` between label and input.
- Required fields are indicated by a leading asterisk in the label text (the design system has no dedicated required-field pattern).
- Error messages render in `text-sm text-destructive` below the input.
- Forms wire state with `useState` + `onChange` — `react-hook-form` is not installed.

## Accessibility

- Every interactive element must be keyboard-focusable. shadcn primitives handle this — do not `tabIndex={-1}` a button.
- Focus rings: let the primitive style apply. Do not add custom `focus:ring-*` unless overriding a broken primitive.
- Every `<button>` that is icon-only must have `aria-label` or a wrapping `<Tooltip>` with text content.
- Do not use `title` attributes as the only label — screen readers treat them inconsistently.
- Color alone must not convey meaning — pair with an icon or text label.

## What NOT to do

- Do not add new CSS files. All styling goes through Tailwind classes or the shadcn primitive layer.
- Do not add a new CSS variable. Use existing tokens or ask.
- Do not install a new component library. shadcn + existing primitives cover the surface.
- Do not use `style={{ ... }}` for dynamic values. If you need runtime CSS (e.g. animation progress), set CSS variables via `style={{ '--progress': `${n}%` }}` and consume them in a class. The only legitimate use of inline `style` is passing a dynamic CSS variable.
- Do not use third-party styling libraries (`styled-components`, `emotion`, CSS modules). Tailwind + CVA only.
- Do not write keyframes. Use `tailwindcss-animate` utilities.
- Do not hardcode dimensions for decorative elements (`w-[12px]`). Use the scale.
- Do not introduce a "theme provider" or "design tokens package." The CSS variable system IS the tokens.

## When the design spec diverges from the system

If a spec (e.g. `docs/superpowers/specs/*.md`) asks for something the current design system does not support (a new color, a new radius, a new font size, a new primitive), **stop and ask the user**. Do not silently add CSS variables or arbitrary values. The contract review flagged this exact drift — `ContractPanel` had to fall back from `sheet` to `dialog` because `sheet` was not installed, and the deviation was documented rather than hidden.

Prefer to:
1. Find an existing token or primitive that is close enough.
2. Document the deviation in a code comment or feature doc.
3. Only add to the design system after user approval.
