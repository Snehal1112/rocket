import type { GitStatusKind } from '@/lib/tauri-api';

// ── HTTP Method Colors (jewel tones) ────────────────────────────────

/** Text-only color for method labels in compact contexts (tree nodes, tabs). */
export const METHOD_TEXT_COLOR: Record<string, string> = {
  GET: 'text-emerald-500 dark:text-emerald-400',
  POST: 'text-amber-500 dark:text-amber-400',
  PUT: 'text-blue-500 dark:text-blue-400',
  PATCH: 'text-violet-500 dark:text-violet-400',
  DELETE: 'text-red-500 dark:text-red-400',
  OPTIONS: 'text-cyan-500 dark:text-cyan-400',
  HEAD: 'text-pink-500 dark:text-pink-400',
};

/** Full badge color (text + bg + border) for method badges with backgrounds. */
export const METHOD_BADGE_COLOR: Record<string, string> = {
  GET: 'text-emerald-500 dark:text-emerald-400 border-emerald-500/30 bg-emerald-500/10 dark:bg-emerald-500/20',
  POST: 'text-amber-500 dark:text-amber-400 border-amber-500/30 bg-amber-500/10 dark:bg-amber-500/20',
  PUT: 'text-blue-500 dark:text-blue-400 border-blue-500/30 bg-blue-500/10 dark:bg-blue-500/20',
  PATCH:
    'text-violet-500 dark:text-violet-400 border-violet-500/30 bg-violet-500/10 dark:bg-violet-500/20',
  DELETE: 'text-red-500 dark:text-red-400 border-red-500/30 bg-red-500/10 dark:bg-red-500/20',
  OPTIONS: 'text-cyan-500 dark:text-cyan-400 border-cyan-500/30 bg-cyan-500/10 dark:bg-cyan-500/20',
  HEAD: 'text-pink-500 dark:text-pink-400 border-pink-500/30 bg-pink-500/10 dark:bg-pink-500/20',
};

/** Solid bg color for chart bars and progress indicators. */
export const METHOD_CHART_COLOR: Record<string, { text: string; bg: string }> = {
  GET: { text: 'text-emerald-500 dark:text-emerald-400', bg: 'bg-emerald-500' },
  POST: { text: 'text-amber-500 dark:text-amber-400', bg: 'bg-amber-500' },
  PUT: { text: 'text-blue-500 dark:text-blue-400', bg: 'bg-blue-500' },
  PATCH: { text: 'text-violet-500 dark:text-violet-400', bg: 'bg-violet-500' },
  DELETE: { text: 'text-red-500 dark:text-red-400', bg: 'bg-red-500' },
  OPTIONS: { text: 'text-cyan-500 dark:text-cyan-400', bg: 'bg-cyan-500' },
  HEAD: { text: 'text-pink-500 dark:text-pink-400', bg: 'bg-pink-500' },
};

// ── HTTP Status Code Colors ─────────────────────────────────────────

/** Text-only color for status codes in compact contexts. */
export function statusTextColor(status: number): string {
  if (status >= 500 || status === 0) return 'text-red-600 dark:text-red-400';
  if (status >= 400) return 'text-amber-600 dark:text-amber-400';
  if (status >= 300) return 'text-blue-600 dark:text-blue-400';
  if (status >= 200) return 'text-emerald-600 dark:text-emerald-400';
  return 'text-muted-foreground';
}

/** Full badge color for status code badges (text + bg + border). */
export function statusBadgeColor(status: number): string {
  if (status >= 500)
    return 'bg-red-100 text-red-700 border-red-200 dark:bg-red-900/30 dark:text-red-400 dark:border-red-800';
  if (status >= 400)
    return 'bg-amber-100 text-amber-700 border-amber-200 dark:bg-amber-900/30 dark:text-amber-400 dark:border-amber-800';
  if (status >= 300)
    return 'bg-blue-100 text-blue-700 border-blue-200 dark:bg-blue-900/30 dark:text-blue-400 dark:border-blue-800';
  if (status >= 200)
    return 'bg-emerald-100 text-emerald-700 border-emerald-200 dark:bg-emerald-900/30 dark:text-emerald-400 dark:border-emerald-800';
  return 'bg-red-100 text-red-700 border-red-200 dark:bg-red-900/30 dark:text-red-400 dark:border-red-800';
}

/** Color for response time indicators. */
export function timeColor(ms: number): string {
  if (ms <= 200) return 'text-emerald-600 dark:text-emerald-400';
  if (ms <= 1000) return 'text-amber-600 dark:text-amber-400';
  return 'text-red-600 dark:text-red-400';
}

// ── Git Status Colors ───────────────────────────────────────────────

export const GIT_STATUS_CONFIG: Record<GitStatusKind, { label: string; className: string }> = {
  modified: { label: 'M', className: 'text-git-modified' },
  added: { label: 'A', className: 'text-git-added' },
  deleted: { label: 'D', className: 'text-git-deleted' },
  renamed: { label: 'R', className: 'text-git-modified' },
  untracked: { label: 'U', className: 'text-git-untracked' },
  conflicted: { label: 'C', className: 'text-git-conflicted' },
  unchanged: { label: '', className: '' },
};

// ── VSCode 2026 Chart Palette ───────────────────────────────────────

/** Named chart colors from VSCode 2026 token set — use for non-method chart elements. */
export const CHART_COLORS = {
  blue: 'text-chart-1',
  green: 'text-chart-green',
  orange: 'text-chart-orange',
  purple: 'text-chart-purple',
  red: 'text-chart-5',
  yellow: 'text-chart-3',
} as const;
