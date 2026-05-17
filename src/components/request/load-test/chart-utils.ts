import type { TimeSeriesPoint } from '@/lib/tauri-api';

/**
 * Formats an elapsed_ms value for chart X-axis labels.
 * Shows milliseconds when the total span is under 2 seconds to avoid
 * all labels collapsing to "0.0" for fast sub-second tests.
 */
export function makeTimeLabel(points: TimeSeriesPoint[]): (elapsedMs: number) => string {
  const maxMs = points.length > 0 ? (points[points.length - 1]?.elapsedMs ?? 0) : 0;
  if (maxMs < 2000) {
    return (ms) => `${ms}ms`;
  }
  return (ms) => (ms / 1000).toFixed(1);
}
