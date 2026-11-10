import type { ContractCheckOutcome, ContractVerdict } from '@/lib/tauri-api';

export const NO_ANSWER_TEXT = 'No answer yet';

export function verdictLabel(verdict: ContractVerdict): string {
  switch (verdict) {
    case 'pass':
      return 'Pass';
    case 'break':
      return 'Break';
    case 'not_used':
      return 'Not used';
    default:
      // A verdict added later gets a neutral label.
      return 'Unknown';
  }
}

/** The state text of one node. A node without an entry has not answered yet. */
export function resultStateLabel(outcome: ContractCheckOutcome | 'expired' | undefined): string {
  switch (outcome) {
    case undefined:
      return NO_ANSWER_TEXT;
    case 'answered':
      return 'Answered';
    case 'declined':
      return 'Declined';
    case 'busy':
      return 'Busy';
    case 'expired':
      return 'Expired';
    default:
      return NO_ANSWER_TEXT;
  }
}

// One toast slot per check, so the expiry can dismiss it.
export const contractCheckToastId = (checkId: string): string =>
  `network-contract-check-offer-${checkId}`;

/** The offer toast text. The contract is peer text, so it only goes in as plain text. */
export function offerToastText(
  displayName: string | null,
  contract: string,
  changeCount: number,
): string {
  const name = displayName?.trim() || 'A teammate';
  const changes = changeCount === 1 ? '1 change' : `${changeCount} changes`;
  return `"${name}" wants to check a change to "${contract}" (${changes})`;
}
