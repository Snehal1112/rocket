import { ExternalLink } from 'lucide-react'
import { Button } from '@/components/ui/button'
import type { Contract } from '@/types/contracts'

interface ChangelogDrawerFooterProps {
  contract: Contract
  shownCount: number
}

export function ChangelogDrawerFooter({ contract, shownCount }: ChangelogDrawerFooterProps) {
  const totalCount = contract.changelog.length

  return (
    <div className='px-5 py-2.5 border-t border-border flex justify-between items-center text-[11px] text-muted-foreground'>
      <span>
        Showing {shownCount} of {totalCount}
      </span>
      <div className='flex items-center gap-1'>
        <Button
          variant='ghost'
          size='sm'
          onClick={() => {}}
        >
          Open as tab <ExternalLink className='w-3 h-3 ml-1' aria-hidden='true' />
        </Button>
        <Button variant='ghost' size='sm'>
          Export CSV
        </Button>
      </div>
    </div>
  )
}
