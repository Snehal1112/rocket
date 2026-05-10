import { useState, useEffect } from 'react'
import { AlertTriangle } from 'lucide-react'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import { useDrawerStore } from '@/stores/contracts/drawerSlice'
import type { Contract } from '@/types/contracts'

interface ChangelogDrawerToolbarProps {
  contract: Contract
}

interface PillProps {
  children: React.ReactNode
  isActive?: boolean
  onClick?: () => void
}

function Pill({ children, isActive, onClick }: PillProps) {
  return (
    <Button
      variant='ghost'
      onClick={onClick}
      aria-pressed={isActive}
      className={cn(
        'text-xs px-2.5 h-[22px] rounded-xl border inline-flex items-center gap-1 select-none',
        isActive
          ? 'bg-primary/15 text-primary border-primary/30 hover:bg-primary/20'
          : 'border-border text-muted-foreground hover:bg-accent',
      )}
    >
      {children}
    </Button>
  )
}

export function ChangelogDrawerToolbar({ contract }: ChangelogDrawerToolbarProps) {
  const filters = useDrawerStore(s => s.filters)
  const setSearch = useDrawerStore(s => s.setSearch)
  const toggleKind = useDrawerStore(s => s.toggleKind)
  const toggleBreakingOnly = useDrawerStore(s => s.toggleBreakingOnly)
  const toggleSinceSigned = useDrawerStore(s => s.toggleSinceSigned)
  const resetFilters = useDrawerStore(s => s.resetFilters)

  const [localSearch, setLocalSearch] = useState(filters.search)

  useEffect(() => {
    const t = setTimeout(() => setSearch(localSearch), 200)
    return () => clearTimeout(t)
  }, [localSearch, setSearch])

  useEffect(() => {
    setLocalSearch(filters.search)
  }, [filters.search])

  const total = contract.changelog.length
  const breakingCount = contract.changelog.filter(e => e.isBreaking).length
  const addCount = contract.changelog.filter(e => e.kind === 'add').length
  const removeCount = contract.changelog.filter(e => e.kind === 'remove').length
  const modifyCount = contract.changelog.filter(e => e.kind === 'modify').length

  const isAllActive = filters.kinds.length === 0 && !filters.breakingOnly && !filters.sinceSigned && filters.search === ''

  return (
    <div className='px-5 py-2.5 border-b border-border flex gap-2 items-center flex-wrap'>
      <Input
        placeholder='Search changes…'
        value={localSearch}
        onChange={e => setLocalSearch(e.target.value)}
        className='flex-1 min-w-[160px] bg-card border border-border rounded-sm px-2.5 h-7 text-xs outline-none placeholder:text-muted-foreground/60'
      />
      <Pill isActive={isAllActive} onClick={resetFilters}>
        All · {total}
      </Pill>
      {breakingCount > 0 && (
        <Pill isActive={filters.breakingOnly} onClick={toggleBreakingOnly}>
          <AlertTriangle className='w-3 h-3' aria-hidden='true' />
          Breaking · {breakingCount}
        </Pill>
      )}
      <Pill isActive={filters.kinds.includes('remove')} onClick={() => toggleKind('remove')}>
        REM · {removeCount}
      </Pill>
      <Pill isActive={filters.kinds.includes('add')} onClick={() => toggleKind('add')}>
        ADD · {addCount}
      </Pill>
      <Pill isActive={filters.kinds.includes('modify')} onClick={() => toggleKind('modify')}>
        MOD · {modifyCount}
      </Pill>
      <Pill isActive={filters.sinceSigned} onClick={toggleSinceSigned}>
        Since signed
      </Pill>
    </div>
  )
}
