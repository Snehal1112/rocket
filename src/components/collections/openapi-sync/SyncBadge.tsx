import type { ComponentProps } from 'react';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';

export type SyncTone = 'neutral' | 'info' | 'success' | 'warning' | 'danger';

// Soft tints with a matching border read on both themes. The solid secondary badge is nearly
// the same shade as the dark background, so it does not.
const TONE: Record<SyncTone, string> = {
  neutral: 'border-foreground/15 bg-foreground/10 text-foreground/80',
  info: 'border-sky-500/30 bg-sky-500/15 text-sky-700 dark:text-sky-300',
  success: 'border-emerald-500/30 bg-emerald-500/15 text-emerald-700 dark:text-emerald-300',
  warning: 'border-amber-500/35 bg-amber-500/15 text-amber-800 dark:text-amber-300',
  danger: 'border-red-500/35 bg-red-500/15 text-red-700 dark:text-red-300',
};

const DOT: Record<SyncTone, string> = {
  neutral: 'bg-foreground/50',
  info: 'bg-sky-500',
  success: 'bg-emerald-500',
  warning: 'bg-amber-500',
  danger: 'bg-red-500',
};

interface SyncBadgeProps extends Omit<ComponentProps<typeof Badge>, 'variant'> {
  tone?: SyncTone;
  /** Shows a small status dot before the text. */
  dot?: boolean;
}

/** A badge for the OpenAPI sync tab: a tinted pill that stays readable on light and dark. */
export function SyncBadge({
  tone = 'neutral',
  dot = false,
  className,
  children,
  ...props
}: SyncBadgeProps) {
  return (
    <Badge
      variant='outline'
      className={cn('gap-1.5 rounded-full px-2 py-0 text-xs font-medium', TONE[tone], className)}
      {...props}
    >
      {dot && <span aria-hidden='true' className={cn('h-1.5 w-1.5 rounded-full', DOT[tone])} />}
      {children}
    </Badge>
  );
}

const METHOD_COLOR: Record<string, string> = {
  GET: 'text-[hsl(var(--color-method-get))]',
  POST: 'text-[hsl(var(--color-method-post))]',
  PUT: 'text-[hsl(var(--color-method-put))]',
  DELETE: 'text-[hsl(var(--color-method-delete))]',
  PATCH: 'text-[hsl(var(--color-method-patch))]',
  OPTIONS: 'text-[hsl(var(--color-method-options))]',
  HEAD: 'text-[hsl(var(--color-method-head))]',
};

/** An HTTP method in the app's method color. */
export function MethodLabel({ method, className }: { method: string; className?: string }) {
  return (
    <span
      className={cn(
        'font-mono text-xs font-semibold',
        METHOD_COLOR[method.toUpperCase()] ?? 'text-foreground',
        className,
      )}
    >
      {method}
    </span>
  );
}

const INK = 'text-[#111827]';
const DARK_INK = 'dark:text-[#111827]';

/**
 * The pill fill and text per method. Each pairing reaches 4.5:1 in both themes (a test checks it).
 * HEAD in light and DELETE in dark use their own lightness because the shared token fails both
 * white and dark text there.
 */
export const METHOD_FILL: Record<string, string> = {
  GET: `bg-[hsl(var(--color-method-get))] ${INK} ${DARK_INK}`,
  POST: `bg-[hsl(var(--color-method-post))] ${INK} ${DARK_INK}`,
  PUT: `bg-[hsl(var(--color-method-put))] text-white ${DARK_INK}`,
  DELETE:
    'bg-[hsl(var(--color-method-delete))] text-white dark:bg-[hsl(0_72%_62%)] dark:text-[#111827]',
  PATCH: `bg-[hsl(var(--color-method-patch))] text-white ${DARK_INK}`,
  OPTIONS: `bg-[hsl(var(--color-method-options))] ${INK} ${DARK_INK}`,
  HEAD: 'bg-[hsl(330_65%_49%)] text-white dark:bg-[hsl(var(--color-method-head))] dark:text-[#111827]',
};

/** The tinted bar background and border of an operation row, one set per method. */
export const METHOD_BAR: Record<string, string> = {
  GET: 'bg-[hsl(var(--color-method-get)/0.1)] border-[hsl(var(--color-method-get)/0.4)] hover:bg-[hsl(var(--color-method-get)/0.18)]',
  POST: 'bg-[hsl(var(--color-method-post)/0.1)] border-[hsl(var(--color-method-post)/0.4)] hover:bg-[hsl(var(--color-method-post)/0.18)]',
  PUT: 'bg-[hsl(var(--color-method-put)/0.1)] border-[hsl(var(--color-method-put)/0.4)] hover:bg-[hsl(var(--color-method-put)/0.18)]',
  DELETE:
    'bg-[hsl(var(--color-method-delete)/0.1)] border-[hsl(var(--color-method-delete)/0.4)] hover:bg-[hsl(var(--color-method-delete)/0.18)]',
  PATCH:
    'bg-[hsl(var(--color-method-patch)/0.1)] border-[hsl(var(--color-method-patch)/0.4)] hover:bg-[hsl(var(--color-method-patch)/0.18)]',
  OPTIONS:
    'bg-[hsl(var(--color-method-options)/0.1)] border-[hsl(var(--color-method-options)/0.4)] hover:bg-[hsl(var(--color-method-options)/0.18)]',
  HEAD: 'bg-[hsl(var(--color-method-head)/0.1)] border-[hsl(var(--color-method-head)/0.4)] hover:bg-[hsl(var(--color-method-head)/0.18)]',
};

/** The bar classes for a method, with a neutral fallback. */
export function methodBarClass(method: string): string {
  return METHOD_BAR[method.toUpperCase()] ?? 'bg-muted/40 border-border';
}

/** A solid method pill with a text color chosen for contrast, a fixed width. */
export function MethodPill({ method, className }: { method: string; className?: string }) {
  return (
    <span
      className={cn(
        'inline-flex w-16 shrink-0 items-center justify-center rounded px-1.5 py-1 font-mono text-xs font-bold',
        METHOD_FILL[method.toUpperCase()] ?? 'bg-foreground/60 text-white',
        className,
      )}
    >
      {method}
    </span>
  );
}
