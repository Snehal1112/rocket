import { isActive, isInactive, needsAttention } from '@/lib/contracts/statusMachine';
import type { Contract, ContractCounts, ContractStatus } from '@/types/contracts';

export function selectContractsForCollection(
  byId: Record<string, Contract>,
  byCollection: Record<string, string[]>,
  collectionId: string,
): Contract[] {
  const ids = byCollection[collectionId] ?? [];
  return ids.map((id) => byId[id]).filter(Boolean);
}

export function selectContractCounts(contracts: Contract[]): ContractCounts {
  let active = 0,
    drift = 0,
    breach = 0,
    inReview = 0,
    draft = 0,
    paused = 0,
    expired = 0,
    archived = 0;
  let totalChanges = 0,
    changesAdded = 0,
    changesRemoved = 0,
    changesModified = 0;

  for (const c of contracts) {
    if (isActive(c.status)) active++;
    else if (c.status === 'drift') drift++;
    else if (c.status === 'breach') breach++;
    else if (c.status === 'in_review') inReview++;
    else if (c.status === 'draft') draft++;
    else if (c.status === 'paused') paused++;
    else if (c.status === 'expired') expired++;
    else if (c.status === 'archived') archived++;

    totalChanges += c.driftCount;
    for (const entry of c.changelog) {
      if (entry.kind === 'add') changesAdded++;
      else if (entry.kind === 'remove') changesRemoved++;
      else changesModified++;
    }
  }

  return {
    total: contracts.length,
    active,
    drift,
    breach,
    inReview,
    draft,
    paused,
    expired,
    archived,
    totalChanges,
    changesAdded,
    changesRemoved,
    changesModified,
  };
}

export function groupContracts(contracts: Contract[]): {
  attention: Contract[];
  active: Contract[];
  inactive: Contract[];
  archived: Contract[];
} {
  return {
    attention: contracts.filter((c) => needsAttention(c.status)),
    active: contracts.filter((c) => isActive(c.status)),
    inactive: contracts.filter((c) => isInactive(c.status)),
    archived: contracts.filter((c) => c.status === 'archived'),
  };
}

const STATUS_ORDER: Partial<Record<ContractStatus, number>> = {
  breach: 0,
  drift: 1,
  in_review: 2,
  active: 3,
  expiring_in_30_days: 4,
  draft: 5,
  paused: 6,
  expired: 7,
  archived: 8,
};

export function sortContractsAttentionFirst(contracts: Contract[]): Contract[] {
  return [...contracts].sort((a, b) => {
    const orderDiff = (STATUS_ORDER[a.status] ?? 9) - (STATUS_ORDER[b.status] ?? 9);
    if (orderDiff !== 0) return orderDiff;
    return b.updatedAt.localeCompare(a.updatedAt);
  });
}
