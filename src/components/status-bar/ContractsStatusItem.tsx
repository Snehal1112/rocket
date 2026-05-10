import { Lock } from 'lucide-react';
import { useMemo } from 'react';
import { Button } from '@/components/ui/button';
import { track } from '@/lib/telemetry';
import { useWorkspaces } from '@/lib/queries/workspace-queries';
import { useDrawerStore } from '@/stores/contracts/drawerSlice';
import { useContractsStore } from '@/stores/contracts/contractsSlice';
import { usePaneStore } from '@/stores/pane-store';
import { useWorkspaceStore } from '@/stores/workspace-store';

/**
 * Status bar chip: "{n} contracts · {n} drifting · {n} breaching"
 * Shows separators before AND after (spec §9).
 * Renders nothing when no active collection or no contracts exist.
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
    const ids = byCollection[collectionRoot] ?? [];
    const contracts = ids.map((id) => byId[id]).filter(Boolean);
    if (contracts.length === 0) return null;

    const mostRecentId = contracts.reduce<string | null>((best, c) => {
      const latestAt = c.changelog[0]?.at ?? c.updatedAt;
      if (!best) return c.id;
      const bestContract = byId[best];
      const bestAt = bestContract?.changelog[0]?.at ?? bestContract?.updatedAt ?? '';
      return latestAt > bestAt ? c.id : best;
    }, null);

    return {
      total: contracts.length,
      driftCount: contracts.filter((c) => c.status === 'drift').length,
      breachCount: contracts.filter((c) => c.status === 'breach').length,
      mostRecentId,
    };
  }, [byId, byCollection, collectionRoot]);

  if (!meta || !collectionRoot || !activeCollection) return null;

  return (
    <>
      {/* Separator BEFORE (spec §9) */}
      <span className='mx-1 text-muted-foreground/30 select-none' aria-hidden='true'>
        |
      </span>

      <Button
        variant='ghost'
        size='sm'
        className='flex items-center gap-[5px] text-[11px] text-muted-foreground hover:text-foreground transition-colors'
        onClick={() => {
          if (meta.mostRecentId) {
            try { track('contracts.changelog_drawer_opened', { contractId: meta.mostRecentId, source: 'status_bar' }) } catch {}
            openDrawer(meta.mostRecentId);
          }
        }}
        aria-label={`${meta.total} contract${meta.total !== 1 ? 's' : ''}`}
      >
        <Lock className='w-[11px] h-[11px]' aria-hidden='true' />
        <span>
          {meta.total} contract{meta.total !== 1 ? 's' : ''}
        </span>
        {meta.driftCount > 0 && (
          <span className='text-[hsl(var(--warning))]'>· {meta.driftCount} drifting</span>
        )}
        {meta.breachCount > 0 && (
          <span className='text-[hsl(var(--destructive))]'>· {meta.breachCount} breaching</span>
        )}
      </Button>

      {/* Separator AFTER (spec §9) */}
      <span className='mx-1 text-muted-foreground/30 select-none' aria-hidden='true'>
        |
      </span>
    </>
  );
}
