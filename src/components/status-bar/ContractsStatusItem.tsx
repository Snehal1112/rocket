import { Lock } from 'lucide-react';
import { useMemo } from 'react';
import { Button } from '@/components/ui/button';
import { useWorkspaces } from '@/lib/queries/workspace-queries';
import { track } from '@/lib/telemetry';
import { useContractsStore } from '@/stores/contracts/contractsSlice';
import { useDrawerStore } from '@/stores/contracts/drawerSlice';
import { usePaneStore } from '@/stores/pane-store';
import { useWorkspaceStore } from '@/stores/workspace-store';

/**
 * Status bar chip: "{n} contracts · {n} drifting · {n} breaching"
 * Clicking opens the changelog drawer for the most recently updated contract.
 * Renders nothing when there is no active collection or it has no contracts.
 * Self-contained — derives collection root from pane + workspace store.
 */
export function ContractsStatusItem() {
  const activeCollection = usePaneStore((s) => s.activeCollection);
  const activeWorkspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
  const { data: workspaces = [] } = useWorkspaces();
  const activeWorkspace = workspaces.find((w) => w.id === activeWorkspaceId);

  const collectionRoot =
    activeWorkspace && activeCollection
      ? `${activeWorkspace.path}/collections/${activeCollection}`
      : null;

  const openDrawer = useDrawerStore((s) => s.open);
  const byId = useContractsStore((s) => s.byId);
  const byCollection = useContractsStore((s) => s.byCollection);

  const meta = useMemo(() => {
    if (!collectionRoot) return null;

    let total = 0;
    let driftCount = 0;
    let breachCount = 0;
    let mostRecentId: string | null = null;
    let mostRecentAt = '';

    for (const id of byCollection[collectionRoot] ?? []) {
      const contract = byId[id];
      if (!contract) continue;

      total++;
      if (contract.status === 'drift') driftCount++;
      if (contract.status === 'breach') breachCount++;

      const latestAt = contract.changelog[0]?.at ?? contract.updatedAt;
      if (latestAt > mostRecentAt) {
        mostRecentId = contract.id;
        mostRecentAt = latestAt;
      }
    }

    return total > 0 ? { total, driftCount, breachCount, mostRecentId } : null;
  }, [byId, byCollection, collectionRoot]);

  if (!meta?.mostRecentId) return null;

  const mostRecentId = meta.mostRecentId;
  const contractLabel = `${meta.total} contract${meta.total !== 1 ? 's' : ''}`;
  const statusLabel = [
    contractLabel,
    meta.driftCount > 0 && `${meta.driftCount} drifting`,
    meta.breachCount > 0 && `${meta.breachCount} breaching`,
  ]
    .filter(Boolean)
    .join(', ');

  return (
    <div className='flex h-5 items-center border-x border-statusbar-border px-1'>
      <Button
        variant='ghost'
        size='sm'
        className='h-5 gap-1.25 rounded-sm px-1 text-[11px] text-muted-foreground transition-colors hover:bg-statusbar-item-hover hover:text-foreground'
        onClick={() => {
          try {
            track('contracts.changelog_drawer_opened', {
              contractId: mostRecentId,
              source: 'status_bar',
            });
          } catch {
            /* noop */
          }
          openDrawer(mostRecentId);
        }}
        title='Open latest contract changes'
        aria-label={statusLabel}
      >
        <Lock className='w-2.75 h-2.75' aria-hidden='true' />
        <span>{contractLabel}</span>
        {meta.driftCount > 0 && (
          <span className='text-[hsl(var(--warning))]'>· {meta.driftCount} drifting</span>
        )}
        {meta.breachCount > 0 && (
          <span className='text-[hsl(var(--destructive))]'>· {meta.breachCount} breaching</span>
        )}
      </Button>
    </div>
  );
}
