import type { ReferenceItem } from '@/lib/assistant/types';

/** Chips one message can carry, the focus chip included. */
export const MAX_CHIPS = 8;

export interface ComposerChip {
  key: string;
  item: ReferenceItem;
  /** True for the chip that names the open request. */
  focus: boolean;
}

export type AddChipOutcome = 'added' | 'duplicate' | 'limit';

/** Identifies what a chip points at, so the same item is not added twice. */
export function chipKey(item: ReferenceItem): string {
  // A JSON tuple, so a name that holds a colon cannot make two items share a key.
  return JSON.stringify([item.kind, item.collection, item.path ?? '']);
}

/** Adds a chip unless it is already there or the message is full. */
export function addChip(
  chips: ComposerChip[],
  item: ReferenceItem,
  focus = false,
): { chips: ComposerChip[]; outcome: AddChipOutcome } {
  const key = chipKey(item);
  if (chips.some((chip) => chip.key === key)) return { chips, outcome: 'duplicate' };
  if (chips.length >= MAX_CHIPS) return { chips, outcome: 'limit' };
  return { chips: [...chips, { key, item, focus }], outcome: 'added' };
}

export function removeChip(chips: readonly ComposerChip[], key: string): ComposerChip[] {
  return chips.filter((chip) => chip.key !== key);
}
