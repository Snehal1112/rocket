# Frontend design system rules

Rules for implementing UI that stays consistent with the existing Rocket design system. **Do not change the design system itself** — no new CSS variables, no new Tailwind theme keys, no new radius scale, no new fonts. If a design spec asks for something the system cannot express, stop and ask the user.

## Stack (what's actually installed)

- **Tailwind** — `darkMode: ['class']`, configured in `tailwind.config.js`.
- **shadcn/ui** — `new-york` style, `neutral` base, CSS variables, `lucide` icons. Do not `npx shadcn add` without user consent.
- **Fonts** — Inter Variable (body), JetBrains Mono (`font-mono`). Do not import additional fonts.
- **Icons** — Lucide only. Global `stroke-width: 1.5` in `src/globals.css` — do not override per-icon.
- **Animation** — `tailwindcss-animate` / `tw-animate-css`. Use their utilities, never write keyframes.

## Colors

All colors are HSL CSS variables in `src/globals.css`, consumed via Tailwind semantic classes. Mapping is in `tailwind.config.js`. Read those files if you need the full token list.

**Hard rules:**
- Never hardcode hex/rgb. Never introduce a new CSS variable. Never use Tailwind's default palette (`text-blue-500`, etc.).
- Status nuance: use opacity modifiers on existing tokens (`bg-primary/10`), not new tokens.
- Dark mode: semantic tokens switch automatically — only write `dark:` variants for things the token system cannot express (shadows, backdrop). Never `dark:bg-[#...]`.

## Radius and spacing

Use only `rounded-sm` / `rounded-md` / `rounded-lg`. Never `rounded-xl` or arbitrary values.

Use Tailwind's 4px scale (`gap-1` through `gap-6`). No arbitrary spacing (`p-[13px]`).

## Typography

- `text-xs` is the default for dense surfaces in this project — match surrounding components.
- `font-medium` for labels, `font-semibold` for headings, `font-normal` otherwise. No `font-bold`.
- No `tracking-*` overrides unless the spec calls for it.

## Available shadcn primitives

In `src/components/ui/`: `alert-dialog`, `badge`, `button`, `card`, `checkbox`, `collapsible`, `context-menu`, `dialog`, `dropdown-menu`, `input`, `label`, `popover`, `radio-group`, `resizable`, `saved-pill`, `scroll-area`, `select`, `separator`, `table`, `tabs`, `textarea`, `tooltip`, `tree`.

**Not installed — stop and ask before adding:**
- `sheet` — use `dialog` with wider `max-w-*`
- `toast` / `sonner` — no notification system yet; errors go to `console.error`
- `form` / `react-hook-form` — wire forms with `useState` + `onChange`
- `combobox` — use `select` or `popover` + `input`
- `calendar` / `date-picker` — use `<Input type='date'>`

## Component patterns

- **Primitives first.** Compose from shadcn before reaching for raw `<div>`.
- **Do not edit `src/components/ui/`.** Feature variants go in domain folders (`collections/`, `git/`, `contract/`, etc.).
- **New components use CVA** (`class-variance-authority`) + `cn()` from `@/lib/utils`. Always accept `className` as a prop. See any existing component in `src/components/ui/` for the pattern.
- Biome enforces class ordering — accept its re-ordering, never fight it.

## Forms

- Use `<Input>`, `<Textarea>`, `<Select>`, `<Checkbox>`, `<RadioGroup>`, `<Label>`. Never raw `<input>`.
- Labels above inputs with `space-y-1`. Errors in `text-sm text-destructive` below the input.
- Required fields: leading asterisk in the label text.

## Accessibility

- Every icon-only `<button>` needs `aria-label` or a `<Tooltip>`.
- Do not `tabIndex={-1}` a button. Do not use `title` as the only label.
- Color alone must not convey meaning — pair with icon or text.

## When the spec diverges from the system

Stop and ask. Do not silently add CSS variables or arbitrary values. Find an existing token or primitive that is close enough, document the deviation, and only extend the system after user approval.
