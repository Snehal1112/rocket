import { useDrawerStore } from '@/stores/contracts/drawerSlice'
import { useContractsStore } from '@/stores/contracts/contractsSlice'

export function useChangelogDrawer() {
  const isOpen = useDrawerStore(s => s.isOpen)
  const contractId = useDrawerStore(s => s.contractId)
  const open = useDrawerStore(s => s.open)
  const close = useDrawerStore(s => s.close)
  const clearContract = useDrawerStore(s => s.clearContract)
  const contract = useContractsStore(s => contractId ? s.byId[contractId] : null)
  return { isOpen, contract: contract ?? null, open, close, clearContract }
}
