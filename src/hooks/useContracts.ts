import { useMemo } from 'react';
import {
  selectContractCounts,
  selectContractsForCollection,
  sortContractsAttentionFirst,
} from '@/stores/contracts/contractsSelectors';
import { useContractsStore } from '@/stores/contracts/contractsSlice';

export function useContracts(collectionId: string) {
  const byId = useContractsStore((s) => s.byId);
  const byCollection = useContractsStore((s) => s.byCollection);
  const loading = useContractsStore((s) => s.loading);

  const contracts = useMemo(
    () =>
      sortContractsAttentionFirst(selectContractsForCollection(byId, byCollection, collectionId)),
    [byId, byCollection, collectionId],
  );

  const counts = useMemo(() => selectContractCounts(contracts), [contracts]);

  return { contracts, counts, isLoading: loading };
}
