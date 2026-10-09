export interface UsageInput {
  used: number;
  size: number;
  costUsd?: number;
}

export interface UsageView {
  percent: number;
  /** Short text shown in the toolbar, such as `12%`. */
  text: string;
  /** Token counts and cost, shown on hover and to screen readers. */
  detail: string;
}

/** The context-used indicator, or null to hide it (no usage yet, or a zero size). */
export function formatUsage(usage?: UsageInput): UsageView | null {
  if (!usage || usage.size <= 0) return null;
  const percent = Math.min(100, Math.round((usage.used / usage.size) * 100));
  const tokens = `${usage.used.toLocaleString('en-US')} of ${usage.size.toLocaleString('en-US')} tokens`;
  const cost = typeof usage.costUsd === 'number' ? `, $${usage.costUsd.toFixed(4)}` : '';
  return { percent, text: `${percent}%`, detail: `${tokens}${cost}` };
}
