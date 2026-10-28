import { useQueryClient } from '@tanstack/react-query';
import { type ReactNode, useMemo, useState } from 'react';
import { StateBlock } from '@/components/ui/state-block';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import {
  isApplying,
  openApiSyncKeys,
  useApplySync,
  useIsApplying,
  useLastApply,
  useOpenApiSyncChanges,
  useOpenApiSyncPlan,
  useOpenApiSyncStatus,
} from '@/lib/queries/openapi-sync-queries';
import type { OpenApiSyncItem } from '@/lib/tauri-api';
import { ApplyConfirmDialog } from './ApplyConfirmDialog';
import { ApplyResultDialog } from './ApplyResultDialog';
import { BulkConfirmDialog } from './BulkConfirmDialog';
import { buildSelection } from './buildSelection';
import { countOverwrites } from './bulkSelection';
import { CheckFailureBanner } from './CheckFailureBanner';
import { NoUpdatesState } from './NoUpdatesState';
import { OpenApiSpecView } from './OpenApiSpecView';
import { OpenApiSyncActionsBar } from './OpenApiSyncActionsBar';
import { OpenApiSyncChanges } from './OpenApiSyncChanges';
import { OpenApiSyncConsentBanner } from './OpenApiSyncConsentBanner';
import { OpenApiSyncDetail } from './OpenApiSyncDetail';
import { OpenApiSyncHeader } from './OpenApiSyncHeader';
import { OpenApiSyncList } from './OpenApiSyncList';
import { OpenApiSyncOverview } from './OpenApiSyncOverview';
import {
  filterItems,
  filterVariables,
  findEntry,
  firstEntryId,
  isPendingChangedError,
} from './openApiSyncView';
import { GENERIC_SYNC_ERROR, syncErrorMessage } from './syncErrors';
import { useSyncSelection } from './useSyncSelection';

type Links = Array<[string, string]>;

interface OpenApiSyncTabProps {
  collection: string;
}

/** What the pending spec would change, with the choices to apply it. It never runs a check. */
export function OpenApiSyncTab({ collection }: OpenApiSyncTabProps) {
  const status = useOpenApiSyncStatus(collection);
  const planEnabled = !!status.data?.hasPending;
  const [links, setLinks] = useState<Links>([]);
  const plan = useOpenApiSyncPlan(collection, planEnabled, links);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [query, setQuery] = useState('');
  const [confirmOpen, setConfirmOpen] = useState(false);
  // The plan the confirm dialog was opened on, so a different plan closes it.
  const [confirmPlan, setConfirmPlan] = useState<unknown>(null);
  const [busyNotice, setBusyNotice] = useState(false);
  const [checking, setChecking] = useState(false);
  const [tab, setTab] = useState<string | null>(null);
  const [settled, setSettled] = useState(false);
  const [viewingSpec, setViewingSpec] = useState(false);
  const queryClient = useQueryClient();
  const changes = useOpenApiSyncChanges(collection);
  const apply = useApplySync(collection);
  const applyingAnywhere = useIsApplying(collection);
  const { last, dismiss } = useLastApply(collection);

  // The first entry is selected by default, and again when the chosen one is gone.
  const planData = plan.data;
  const sel = useSyncSelection(planData, links, setLinks, collection);
  const { toggle, toggleVariable, approved, variables } = sel;
  // The dialog remembers the plan it was opened for, so it never acts on another one.
  const [bulk, setBulk] = useState<{ kind: 'accept' | 'skip'; plan: unknown } | null>(null);
  const locked = plan.isPlaceholderData;
  // Changed requests where the user and the spec changed the same field.
  const [conflictsOnly, setConflictsOnly] = useState(false);
  const hasConflict = (i: OpenApiSyncItem) =>
    i.kind === 'changed' && i.changes.some((c) => c.outcome === 'conflict');
  const conflictCount = planData ? planData.items.filter(hasConflict).length : 0;
  const listPlan = useMemo(
    () =>
      planData && conflictsOnly && conflictCount > 0
        ? { ...planData, items: planData.items.filter(hasConflict), variableChanges: [] }
        : planData,
    [planData, conflictsOnly, conflictCount],
  );
  const busy = apply.isPending || applyingAnywhere || plan.isFetching;

  const approvedItems = useMemo(
    () => (planData ? planData.items.filter((i) => approved.has(i.key)) : []),
    [planData, approved],
  );

  // A new plan, a locked tab or a running apply closes the bulk dialog for good.
  if (bulk && (bulk.plan !== planData || locked || busy)) setBulk(null);

  // A new plan or an emptied selection closes the confirm dialog for good, so the user never
  // applies a list they did not see.
  if (confirmOpen && (confirmPlan !== planData || approvedItems.length + variables.size === 0)) {
    setConfirmOpen(false);
  }

  // The notice goes away once the other apply is done.
  if (busyNotice && !applyingAnywhere) setBusyNotice(false);

  const confirmApply = () => {
    setConfirmOpen(false);
    if (isApplying(collection)) {
      setBusyNotice(true);
      return;
    }
    const sentLinks = sel.selection.renameLinks.length > 0;
    apply.mutate(sel.selection, {
      // The links are used up once they are written, so the next apply must not resend them.
      onSuccess: () => {
        if (sentLinks) sel.clearLinks();
      },
    });
  };
  const skipSelected = () => {
    if (!planData) return;
    const keys = approvedItems.map((i) => i.key);
    if (keys.length === 0) return;
    if (isApplying(collection)) {
      setBusyNotice(true);
      return;
    }
    // Only the skips go out, with no links. Other approvals wait for Confirm and sync.
    const selection = buildSelection(planData, {
      approved: new Set(),
      choices: new Map(),
      skips: new Set(keys),
      variables: new Set(),
      links: [],
    });
    sel.skip(keys);
    apply.mutate(selection, { onError: () => sel.unskip(keys) });
  };
  // Removed requests are left out, so a search never selects a deletion.
  const selectMatches = () => {
    if (!planData) return;
    const shown = listPlan ?? planData;
    const keys = filterItems(shown.items, query)
      .filter((i) => i.kind !== 'removed')
      .map((i) => i.key);
    sel.setMany(keys, true);
    sel.setManyVariables(
      filterVariables(shown.variableChanges, query).map((v) => v.name),
      true,
    );
  };
  const shownPlan = listPlan ?? planData;
  const matchCount = shownPlan
    ? filterItems(shownPlan.items, query).filter((i) => i.kind !== 'removed').length +
      filterVariables(shownPlan.variableChanges, query).length
    : 0;
  const skipAll = () => {
    setBulk(null);
    if (!planData) return;
    const keys = planData.items.map((i) => i.key);
    if (keys.length === 0) return;
    if (isApplying(collection)) {
      setBusyNotice(true);
      return;
    }
    // Only the skips go out, with no links and no approvals.
    const selection = buildSelection(planData, {
      approved: new Set(),
      choices: new Map(),
      skips: new Set(keys),
      variables: new Set(),
      links: [],
    });
    // A failed skip puts the earlier selection back.
    const undo = sel.skipAll();
    apply.mutate(selection, { onError: undo });
  };
  // A warning comes first when Accept all would overwrite edits. Skip all always asks first.
  const requestAcceptAll = () => {
    if (!planData) return;
    if (!sel.keepMine && countOverwrites(planData) > 0) setBulk({ kind: 'accept', plan: planData });
    else sel.acceptAll();
  };
  const acceptAll = () => {
    setBulk(null);
    sel.acceptAll();
  };
  const checkAgain = async () => {
    setChecking(true);
    try {
      await Promise.all([
        queryClient.refetchQueries({ queryKey: openApiSyncKeys.statusAll(collection) }),
        queryClient.refetchQueries({ queryKey: openApiSyncKeys.planAll(collection) }),
      ]);
    } finally {
      setChecking(false);
      dismiss();
    }
  };

  const activeId =
    shownPlan && findEntry(shownPlan, selectedId)
      ? selectedId
      : shownPlan
        ? firstEntryId(shownPlan)
        : null;

  // The viewer must not wait to reopen by itself when the collection is disconnected and
  // connected again in this tab.
  if (viewingSpec && (status.isError || !status.data)) setViewingSpec(false);

  if (status.isError) {
    return (
      <div className='p-4'>
        <StateBlock tone='error'>
          {syncErrorMessage(status.error) === GENERIC_SYNC_ERROR
            ? GENERIC_SYNC_ERROR
            : `The sync state file cannot be read: ${syncErrorMessage(status.error)}`}
        </StateBlock>
      </div>
    );
  }
  if (status.isPending) {
    return (
      <div className='p-4'>
        <StateBlock tone='loading'>Loading OpenAPI sync...</StateBlock>
      </div>
    );
  }
  const s = status.data;
  if (!s) {
    return (
      <div className='p-4'>
        <StateBlock tone='empty'>This collection is not connected to an OpenAPI spec.</StateBlock>
      </div>
    );
  }

  const noUpdates = <NoUpdatesState collection={collection} status={s} />;
  let body: ReactNode;
  if (!planEnabled) {
    body = noUpdates;
  } else if (plan.isError) {
    body = (
      <div className='p-4'>
        <StateBlock
          tone='error'
          action={
            links.length > 0 ? { label: 'Undo links', onClick: () => setLinks([]) } : undefined
          }
        >
          {isPendingChangedError(plan.error)
            ? 'The pending spec changed, check again'
            : `The plan cannot be built: ${syncErrorMessage(plan.error)}`}
        </StateBlock>
      </div>
    );
  } else if (!planData) {
    body = (
      <div className='p-4'>
        <StateBlock tone='loading'>Loading the plan...</StateBlock>
      </div>
    );
  } else if (!planData.pending) {
    // The plan is the truth when it disagrees with the status flag.
    body = noUpdates;
  } else if (planData.items.length + planData.variableChanges.length === 0) {
    // Everything was skipped. The notes about skips, deletions and warnings stay reachable.
    body = (
      <div className='flex min-h-0 flex-1 flex-col'>
        {noUpdates}
        <div className='flex min-h-0 flex-1'>
          <div className='w-72 shrink-0 border-r border-border'>
            <OpenApiSyncList
              collection={collection}
              plan={planData}
              selectedId={null}
              onSelect={setSelectedId}
              query=''
              approved={sel.approved}
              skips={sel.skips}
              variables={sel.variables}
              onToggle={toggle}
              onToggleVariable={toggleVariable}
            />
          </div>
          <div className='min-w-0 flex-1'>
            <OpenApiSyncDetail
              collection={collection}
              plan={planData}
              selectedId={null}
              choices={sel.choices}
              links={links}
              onChoose={sel.setChoice}
              onLink={sel.linkRename}
              onUnlink={sel.unlink}
              linksLocked={plan.isPlaceholderData}
            />
          </div>
        </div>
      </div>
    );
  } else {
    body = (
      <div className='flex min-h-0 flex-1 flex-col'>
        {s.consent !== 'granted' && (
          <p className='px-4 pt-2 text-xs text-muted-foreground'>
            This pending spec came with the collection files. It may not have been checked on this
            machine.
          </p>
        )}
        <OpenApiSyncActionsBar
          query={query}
          onQueryChange={setQuery}
          updateCount={planData.items.length + planData.variableChanges.length}
          selectedCount={sel.selectedCount}
          itemCount={planData.items.length}
          filteredCount={matchCount}
          itemSelectedCount={approvedItems.length}
          locked={locked}
          busy={busy}
          onSelectMatches={selectMatches}
          onSkipSelected={skipSelected}
          onConfirm={() => {
            setConfirmPlan(planData);
            setConfirmOpen(true);
          }}
          onAcceptAll={requestAcceptAll}
          onSkipAll={() => setBulk({ kind: 'skip', plan: planData })}
          onKeepCurrent={sel.keepCurrent}
          keepMine={sel.keepMine}
          onKeepMineChange={sel.setKeepMine}
          conflictCount={conflictCount}
          conflictsOnly={conflictsOnly}
          onToggleConflictsOnly={() => setConflictsOnly((v) => !v)}
        />
        {(busyNotice || (last !== null && last.report === undefined)) && (
          <div className='px-4 pt-2'>
            <StateBlock tone='error'>
              {syncErrorMessage(busyNotice ? 'An apply is already running.' : last?.error)}
            </StateBlock>
          </div>
        )}
        <div className='flex min-h-0 flex-1'>
          <div className='w-72 shrink-0 border-r border-border'>
            <OpenApiSyncList
              collection={collection}
              plan={listPlan ?? planData}
              selectedId={activeId}
              onSelect={setSelectedId}
              query={query}
              approved={approved}
              skips={sel.skips}
              variables={variables}
              onToggle={toggle}
              onToggleVariable={toggleVariable}
            />
          </div>
          <div className='min-w-0 flex-1'>
            <OpenApiSyncDetail
              collection={collection}
              plan={planData}
              selectedId={activeId}
              choices={sel.choices}
              links={links}
              onChoose={sel.setChoice}
              onLink={sel.linkRename}
              onUnlink={sel.unlink}
              linksLocked={plan.isPlaceholderData}
            />
          </div>
        </div>
        {bulk && (
          <BulkConfirmDialog
            kind={bulk.kind}
            count={
              bulk.kind === 'accept'
                ? planData.items.filter((i) => i.kind !== 'removed').length
                : planData.items.length
            }
            overwritten={bulk.kind === 'accept' ? countOverwrites(planData) : undefined}
            onConfirm={bulk.kind === 'accept' ? acceptAll : skipAll}
            onCancel={() => setBulk(null)}
          />
        )}
        <ApplyConfirmDialog
          open={confirmOpen && approvedItems.length + variables.size > 0}
          onOpenChange={setConfirmOpen}
          items={approvedItems}
          variableCount={variables.size}
          choices={sel.choices}
          skippedCount={sel.skips.size}
          onConfirm={confirmApply}
        />
      </div>
    );
  }

  const changeCount = changes.data
    ? changes.data.modified.length +
      changes.data.added.length +
      changes.data.deleted.length +
      changes.data.backups.length
    : 0;
  const waiting = planData?.pending ? planData.items.length + planData.variableChanges.length : 0;
  const showUpdates = waiting > 0 || (planEnabled && plan.isError);
  // No default is chosen while the plan loads, so the tab does not jump when it arrives.
  const choosing = tab === null && !settled && planEnabled && plan.isPending;
  // The default is chosen once, so a later pending copy never switches or unmounts the tabs.
  if (tab === null && !settled && !choosing) {
    setSettled(true);
    setTab(showUpdates ? 'updates' : 'overview');
  }
  const wanted = tab ?? (showUpdates ? 'updates' : 'overview');
  const active = wanted;

  return (
    <div className='flex h-full flex-col overflow-hidden'>
      <OpenApiSyncHeader
        collection={collection}
        status={s}
        waiting={planData ? waiting : planEnabled ? null : 0}
        onViewSpec={() => setViewingSpec(true)}
      />
      {s.consent !== 'granted' && (
        <OpenApiSyncConsentBanner
          key={collection}
          collection={collection}
          location={s.location}
          consent={s.consent}
        />
      )}
      {viewingSpec ? (
        <OpenApiSpecView collection={collection} onBack={() => setViewingSpec(false)} />
      ) : choosing ? (
        <div className='p-4'>
          <StateBlock tone='loading'>Loading the plan...</StateBlock>
        </div>
      ) : (
        <Tabs value={active} onValueChange={setTab} className='flex min-h-0 flex-1 flex-col'>
          <TabsList className='mx-4 mt-3 self-start'>
            <TabsTrigger value='overview'>Overview</TabsTrigger>
            <TabsTrigger value='updates'>
              {waiting > 0 ? `Spec updates (${waiting})` : 'Spec updates'}
            </TabsTrigger>
            <TabsTrigger value='changes'>
              {changeCount > 0 ? `Your changes (${changeCount})` : 'Your changes'}
            </TabsTrigger>
          </TabsList>
          <TabsContent value='overview' className='flex flex-col gap-4 p-4'>
            <CheckFailureBanner collection={collection} status={s} />
            <OpenApiSyncOverview
              collection={collection}
              status={s}
              onReview={() => setTab('updates')}
            />
          </TabsContent>
          <TabsContent value='updates' className='flex min-h-0 flex-1 flex-col'>
            <div className='px-4 pt-3 empty:hidden'>
              <CheckFailureBanner collection={collection} status={s} />
            </div>
            {body}
          </TabsContent>
          <TabsContent value='changes' className='p-4'>
            <OpenApiSyncChanges collection={collection} />
          </TabsContent>
        </Tabs>
      )}
      <ApplyResultDialog
        report={last?.report ?? null}
        variableCount={last?.variableCount ?? 0}
        onClose={dismiss}
        onCheckAgain={checkAgain}
        checking={checking}
      />
    </div>
  );
}
