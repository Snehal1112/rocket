import { formatDistanceToNow } from 'date-fns';
import { Download, Lock, Plus, RefreshCw } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import { useContractDrift } from '@/hooks/useContractDrift';
import { useContracts } from '@/hooks/useContracts';
import { useContractsFilter } from '@/hooks/useContractsFilter';
import { track } from '@/lib/telemetry';
import { groupContracts } from '@/stores/contracts/contractsSelectors';
import { useContractsStore } from '@/stores/contracts/contractsSlice';
import type { ContractAction } from './ContractCard';
import { ContractCard } from './ContractCard';
import { ContractCardSkeleton } from './ContractCardSkeleton';
import { ContractsEmptyState } from './ContractsEmptyState';
import { ContractsFilterBar } from './ContractsFilterBar';
import { ContractsGroupHeader } from './ContractsGroupHeader';
import { ContractsSummaryRow } from './ContractsSummaryRow';
import { NewContractModal } from './NewContractModal';

interface ContractsTabProps {
  /** Collection root path — used as key for all IPC calls */
  collectionId: string;
  collectionName: string;
}

export function ContractsTab({ collectionId, collectionName }: ContractsTabProps) {
  const [modalOpen, setModalOpen] = useState(false);
  const [focusedIdx, setFocusedIdx] = useState(-1);
  const [lastSync, setLastSync] = useState<Date | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);

  const loadContracts = useContractsStore((s) => s.loadContracts);
  const recomputeDrift = useContractsStore((s) => s.recomputeDrift);
  const store = useContractsStore();

  const { contracts, counts, isLoading } = useContracts(collectionId);
  useContractDrift(collectionId);
  const { filtered, filterState, setSearch, toggleStatus, setSort, setView } =
    useContractsFilter(contracts);

  const { attention, active, inactive } = groupContracts(filtered);
  const allCards = [...attention, ...active, ...inactive];

  // Load on mount + when collectionId changes
  useEffect(() => {
    setLoadError(null);
    loadContracts(collectionId)
      .then(() => {
        setLastSync(new Date());
        try {
          track('contracts.tab_opened', {
            collectionId,
            contractCount: useContractsStore.getState().byCollection[collectionId]?.length ?? 0,
          });
        } catch {}
      })
      .catch((err) => setLoadError(String(err)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [collectionId, loadContracts]);

  const handleAction = useCallback(
    async (action: ContractAction, contractId: string) => {
      try { track('contracts.card_action', { contractId, action }); } catch {}
      try {
        switch (action) {
          case 'pause':
            await store.pauseContract(collectionId, contractId);
            break;
          case 'resume':
            await store.resumeContract(collectionId, contractId);
            break;
          case 'delete':
            await store.deleteContract(collectionId, contractId);
            break;
          case 'duplicate':
            await store.duplicateContract(collectionId, contractId);
            break;
          case 'publish':
            await store.publishContract(collectionId, contractId);
            break;
          case 'resign':
            await store.publishContract(collectionId, contractId);
            break;
          case 'send_for_review':
            await store.sendForReview(collectionId, contractId);
            break;
          case 'approve':
            await store.approveContract(collectionId, contractId);
            break;
          case 'reject':
            await store.rejectContract(collectionId, contractId);
            break;
          case 'renew':
            await store.renewContract(collectionId, contractId, null);
            break;
          // 'open', 'edit', 'view_changelog', 'export' → handled by routing/navigation (future SP)
          default:
            break;
        }
      } catch (err) {
        console.error('[ContractsTab] action error:', err);
      }
    },
    [collectionId, store],
  );

  // Refs array for DOM focus — one entry per card across all groups.
  const cardRefs = useRef<(HTMLElement | null)[]>([]);

  // Move DOM focus when j/k changes the focused index.
  useEffect(() => {
    if (focusedIdx >= 0 && cardRefs.current[focusedIdx]) {
      cardRefs.current[focusedIdx]?.focus();
    }
  }, [focusedIdx]);

  // Stable refs so the keydown handler always sees the latest values
  // without needing to re-register the listener.
  const allCardsRef = useRef(allCards);
  const focusedIdxRef = useRef(focusedIdx);
  const handleActionRef = useRef(handleAction);
  useEffect(() => { allCardsRef.current = allCards; });
  useEffect(() => { focusedIdxRef.current = focusedIdx; }, [focusedIdx]);
  useEffect(() => { handleActionRef.current = handleAction; }, [handleAction]);

  // j/k/n/e/p/del hotkeys — scoped to this tab's lifetime.
  useEffect(() => {
    function handler(e: KeyboardEvent) {
      const tag = (e.target as HTMLElement).tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA' || (e.target as HTMLElement).isContentEditable)
        return;
      if (e.metaKey || e.ctrlKey || e.altKey) return;

      const cards = allCardsRef.current;
      const idx = focusedIdxRef.current;

      if (e.key === 'j') {
        e.preventDefault();
        setFocusedIdx((i) => Math.min(i + 1, cards.length - 1));
      } else if (e.key === 'k') {
        e.preventDefault();
        setFocusedIdx((i) => Math.max(0, i - 1));
      } else if (e.key === 'n') {
        e.preventDefault();
        setModalOpen(true);
      } else if (e.key === 'e') {
        e.preventDefault();
        const c = cards[idx];
        if (c) void handleActionRef.current('edit', c.id);
      } else if (e.key === 'p') {
        e.preventDefault();
        const c = cards[idx];
        if (!c) return;
        void handleActionRef.current(c.status === 'paused' ? 'resume' : 'pause', c.id);
      } else if (e.key === 'Delete' || e.key === 'Backspace') {
        e.preventDefault();
        const c = cards[idx];
        if (c) void handleActionRef.current('delete', c.id);
      }
    }
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, []);

  const isEmpty = !isLoading && !loadError && contracts.length === 0;
  const noResults = !isLoading && contracts.length > 0 && filtered.length === 0;

  function lastSyncLabel(): string {
    if (!lastSync) return '';
    return `Last sync ${formatDistanceToNow(lastSync, { addSuffix: true })}`;
  }

  function handleSync() {
    setLoadError(null);
    recomputeDrift(collectionId)
      .then(() => setLastSync(new Date()))
      .catch((err) => setLoadError(String(err)));
  }

  return (
    <div className='flex flex-col h-full bg-background'>
      {/* ── Pane header ─────────────────────────────────── */}
      <div className='flex items-end justify-between gap-4 px-6 pt-[18px] pb-[14px] border-b border-border flex-shrink-0'>
        <div className='flex items-center gap-3'>
          <div
            className='w-9 h-9 rounded-[calc(var(--radius)-2px)] bg-[hsl(var(--primary)/0.1)] text-primary flex items-center justify-center shrink-0'
            aria-hidden='true'
          >
            <Lock className='h-[18px] w-[18px]' />
          </div>
          <div>
            <h1 className='text-xl font-semibold text-foreground leading-tight tracking-[-0.01em]'>
              Contracts
            </h1>
            <div
              className='text-xs text-muted-foreground flex items-center gap-1.5 mt-0.5 flex-wrap'
              aria-label={`${collectionName} / Contracts`}
            >
              <span>{collectionName}</span>
              <span aria-hidden='true'>/</span>
              <span className='text-foreground/70'>Contracts</span>
              <span
                className='w-[3px] h-[3px] rounded-full bg-muted-foreground/40'
                aria-hidden='true'
              />
              <span>
                {counts.total} contract{counts.total !== 1 ? 's' : ''}
              </span>
              {lastSync && (
                <>
                  <span
                    className='w-[3px] h-[3px] rounded-full bg-muted-foreground/40'
                    aria-hidden='true'
                  />
                  <span>{lastSyncLabel()}</span>
                </>
              )}
            </div>
          </div>
        </div>

        <div className='flex items-center gap-2'>
          <Button variant='ghost' size='sm' onClick={handleSync} aria-label='Sync contracts'>
            <RefreshCw className='h-3.5 w-3.5 mr-1.5' aria-hidden='true' />
            Sync
          </Button>
          <Button variant='outline' size='sm' aria-label='Export contracts' disabled>
            <Download className='h-3.5 w-3.5 mr-1.5' aria-hidden='true' />
            Export
          </Button>
          <Button size='sm' onClick={() => setModalOpen(true)}>
            <Plus className='h-3.5 w-3.5 mr-1' aria-hidden='true' />
            New contract
          </Button>
        </div>
      </div>

      {/* ── Error banner ────────────────────────────────── */}
      {loadError && (
        <div
          role='alert'
          className='mx-6 mt-3 px-4 py-3 rounded-md bg-destructive/10 border border-destructive/30 text-sm text-destructive flex items-center justify-between flex-shrink-0'
        >
          <span>Failed to load contracts: {loadError}</span>
          <Button
            variant='ghost'
            size='sm'
            className='h-7 text-xs text-destructive hover:text-destructive'
            onClick={() => {
              setLoadError(null);
              loadContracts(collectionId)
                .then(() => setLastSync(new Date()))
                .catch((e) => setLoadError(String(e)));
            }}
          >
            Retry
          </Button>
        </div>
      )}

      {/* ── Summary row ─────────────────────────────────── */}
      {!isEmpty && !loadError && <ContractsSummaryRow counts={counts} />}

      {/* ── Filter bar ──────────────────────────────────── */}
      {!isEmpty && !loadError && (
        <ContractsFilterBar
          filterState={filterState}
          counts={counts}
          onSearch={(q) => {
            setSearch(q);
            if (q.length > 0) {
              try { track('contracts.filter_used', { filterType: 'search' }); } catch {}
            }
          }}
          onToggleStatus={(s) => {
            toggleStatus(s);
            try { track('contracts.filter_used', { filterType: 'status', value: s }); } catch {}
          }}
          onSetSort={(s) => {
            setSort(s);
            try { track('contracts.filter_used', { filterType: 'sort', value: s }); } catch {}
          }}
          onSetView={setView}
        />
      )}

      {/* ── Content area ────────────────────────────────── */}
      {isEmpty && !loadError ? (
        <ContractsEmptyState
          onStartFromCurrent={() => {
            try { track('contracts.empty_state_cta', { action: 'start_from_current' }); } catch {}
            setModalOpen(true);
          }}
        />
      ) : (
        <ScrollArea className='flex-1'>
          <div className='px-6 py-4'>
            {isLoading ? (
              [1, 2, 3].map((i) => <ContractCardSkeleton key={i} />)
            ) : noResults ? (
              <div className='text-center py-12 text-sm text-muted-foreground'>
                No contracts match your filters.
              </div>
            ) : (
              <>
                {attention.length > 0 && (
                  <>
                    <ContractsGroupHeader label='Needs attention' count={attention.length} />
                    {attention.map((c, i) => (
                      <ContractCard
                        key={c.id}
                        ref={(el) => { cardRefs.current[i] = el; }}
                        contract={c}
                        collectionName={collectionName}
                        collectionRoot={collectionId}
                        onAction={handleAction}
                        focused={focusedIdx === i}
                      />
                    ))}
                  </>
                )}
                {active.length > 0 && (
                  <>
                    <ContractsGroupHeader label='Active' count={active.length} />
                    {active.map((c, i) => (
                      <ContractCard
                        key={c.id}
                        ref={(el) => { cardRefs.current[attention.length + i] = el; }}
                        contract={c}
                        collectionName={collectionName}
                        collectionRoot={collectionId}
                        onAction={handleAction}
                        focused={focusedIdx === attention.length + i}
                      />
                    ))}
                  </>
                )}
                {inactive.length > 0 && (
                  <>
                    <ContractsGroupHeader label='Inactive' count={inactive.length} />
                    {inactive.map((c, i) => (
                      <ContractCard
                        key={c.id}
                        ref={(el) => { cardRefs.current[attention.length + active.length + i] = el; }}
                        contract={c}
                        collectionName={collectionName}
                        collectionRoot={collectionId}
                        onAction={handleAction}
                        focused={focusedIdx === attention.length + active.length + i}
                      />
                    ))}
                  </>
                )}
              </>
            )}
          </div>
        </ScrollArea>
      )}

      {/* ── New contract modal ──────────────────────────── */}
      <NewContractModal
        open={modalOpen}
        onOpenChange={setModalOpen}
        collectionId={collectionId}
        collectionName={collectionName}
      />
    </div>
  );
}
