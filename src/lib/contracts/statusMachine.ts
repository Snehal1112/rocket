import type { ContractStatus, DriftReport } from '@/types/contracts';

/**
 * Determines the next contract status based on a computed DriftReport.
 * Used for optimistic UI updates only — Rust is the authoritative source.
 *
 * Manual transitions (pause/resume/publish/etc.) are handled by the
 * Tauri commands directly and update the store via IPC response.
 */
export function transitionStatus(current: ContractStatus, report: DriftReport): ContractStatus {
  // Paused and draft contracts are not affected by drift
  if (current === 'paused' || current === 'draft') return current;

  if (report.breachCount > 0) return 'breach';
  if (report.driftCount > 0) return 'drift';

  // No changes — if currently drift/breach, revert to active (changes reverted)
  if (current === 'drift' || current === 'breach') return 'active';

  return current;
}

/** Human-readable label for each status. */
export function statusLabel(status: ContractStatus): string {
  const labels: Record<ContractStatus, string> = {
    draft: 'Draft',
    active: 'Active',
    drift: 'Drift',
    breach: 'Breaching',
    in_review: 'In review',
    paused: 'Paused',
    expired: 'Expired',
    expiring_in_30_days: 'Expiring Soon',
  };
  return labels[status] ?? status;
}

/** Returns the display label for use inside ContractStatusChip (includes count for drift/breach). */
export function statusChipLabel(status: ContractStatus, count?: number): string {
  if (status === 'drift' && count && count > 0) return `⚠ Drift · ${count}`;
  if (status === 'breach') return 'Breaching';
  return statusLabel(status);
}

export function needsAttention(status: ContractStatus): boolean {
  return ['breach', 'drift', 'in_review'].includes(status);
}

export function isActive(status: ContractStatus): boolean {
  return status === 'active' || status === 'expiring_in_30_days';
}

export function isInactive(status: ContractStatus): boolean {
  return ['draft', 'paused', 'expired'].includes(status);
}
