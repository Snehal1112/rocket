import type { RepeatUntil } from './tauri-api';

/** Settings a Request node gets when Repeat until is turned on. Matches `RepeatUntil::default()` in rocket-flow. */
export const DEFAULT_REPEAT_UNTIL: RepeatUntil = {
  condition: 'response.status === 200',
  intervalMs: 2000,
  maxAttempts: 30,
  timeoutMs: 60000,
};

/** Formats milliseconds as seconds, e.g. 2000 → "2s", 1500 → "1.5s". */
export function msToSecondsLabel(ms: number): string {
  const seconds = ms / 1000;
  return `${Number.isInteger(seconds) ? seconds : seconds.toFixed(1)}s`;
}
