import { Sheet, SheetContent } from '@/components/ui/sheet'
import { useChangelogEntries } from '@/hooks/useChangelogEntries'
import { useChangelogDrawer } from '@/hooks/useChangelogDrawer'
import type { Contract } from '@/types/contracts'
import { ChangelogDrawerFooter } from './ChangelogDrawerFooter'
import { ChangelogDrawerHeader } from './ChangelogDrawerHeader'
import { ChangelogDrawerToolbar } from './ChangelogDrawerToolbar'
import { ChangelogTimeline } from './ChangelogTimeline'

export function ChangelogDrawer() {
  const { isOpen, contract, close } = useChangelogDrawer()
  if (!contract) return null

  return <ChangelogDrawerInner isOpen={isOpen} contract={contract} onClose={close} />
}

function ChangelogDrawerInner({
  isOpen,
  contract,
  onClose,
}: {
  isOpen: boolean
  contract: Contract
  onClose: () => void
}) {
  const groups = useChangelogEntries(contract)
  const shownCount = groups.reduce((acc, g) => acc + g.items.length, 0)

  return (
    <Sheet open={isOpen} onOpenChange={open => !open && onClose()}>
      <SheetContent
        side='right'
        className='w-[540px] sm:max-w-[540px] p-0 flex flex-col gap-0'
        aria-labelledby='changelog-drawer-title'
      >
        <ChangelogDrawerHeader contract={contract} onClose={onClose} />
        <ChangelogDrawerToolbar contract={contract} />
        <ChangelogTimeline groups={groups} />
        <ChangelogDrawerFooter contract={contract} shownCount={shownCount} />
      </SheetContent>
    </Sheet>
  )
}
