import { AlertTriangle } from 'lucide-react'
import { Input } from '@/components/ui/input'
import { cn } from '@/lib/utils'
import type { Contract } from '@/types/contracts'

interface ChangelogDrawerToolbarProps {
  contract: Contract
}

interface PillProps {
  label: string
  active?: boolean
}

function Pill({ label, active }: PillProps) {
  return (
    <span
      className={cn(
        'text-xs px-2.5 h-[22px] rounded-xl border border-border inline-flex items-center gap-1 select-none',
        active
          ? 'bg-primary/15 text-primary border-primary/30'
          : 'text-muted-foreground hover:bg-accent',
      )}
    >
      {label}
    </span>
  )
}

export function ChangelogDrawerToolbar({ contract }: ChangelogDrawerToolbarProps) {
  const total = contract.changelog.length
  const breakingCount = contract.changelog.filter(e => e.isBreaking).length
  const addCount = contract.changelog.filter(e => e.kind === 'add').length
  const removeCount = contract.changelog.filter(e => e.kind === 'remove').length
  const modifyCount = contract.changelog.filter(e => e.kind === 'modify').length

  return (
    <div className='px-5 py-2.5 border-b border-border flex gap-2 items-center flex-wrap'>
      <Input
        placeholder='Search changes…'
        className='flex-1 min-w-[160px] bg-card border border-border rounded-sm px-2.5 h-7 text-xs outline-none placeholder:text-muted-foreground/60'
      />
      <Pill label={`All · ${total}`} active />
      {breakingCount > 0 && (
        <span
          className={cn(
            'text-xs px-2.5 h-[22px] rounded-xl border border-border inline-flex items-center gap-1 select-none',
            'text-muted-foreground hover:bg-accent',
          )}
        >
          <AlertTriangle className='w-3 h-3' aria-hidden='true' />
          Breaking · {breakingCount}
        </span>
      )}
      <Pill label={`REM · ${removeCount}`} />
      <Pill label={`ADD · ${addCount}`} />
      <Pill label={`MOD · ${modifyCount}`} />
      <Pill label='Since signed' />
    </div>
  )
}
