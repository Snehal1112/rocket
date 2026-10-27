import { useQueryClient } from '@tanstack/react-query';
import { type ReactNode, useCallback, useMemo, useState } from 'react';
import { EmptyState } from '@/components/ui/empty-state';
import { StateBlock } from '@/components/ui/state-block';
import {
  isApplying,
  openApiSyncKeys,
  useApplySync,
  useIsApplying,
  useLastApply,
  useOpenApiSyncEvents,
  useOpenApiSyncPlan,
  useOpenApiSyncStatus,
} from '@/lib/queries/openapi-sync-queries';
import { formatRelativeTime } from '@/lib/relative-time';
import { ApplyConfirmDialog } from './ApplyConfirmDialog';
import { ApplyResultDialog } from './ApplyResultDialog';
import { buildSelection } from './buildSelection';
import { OpenApiSyncActionsBar } from './OpenApiSyncActionsBar';
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
  useOpenApiSyncEvents();
  const status = useOpenApiSyncStatus(collection);
  const planEnabled = !!status.data?.hasPending;
  const [links, setLinks] = useState<Links>([]);
  const plan = useOpenApiSyncPlan(collection, planEnabled, links);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [query, setQuery] = useState('');
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [busyNotice, setBusyNotice] = useState(false);
  const [checking, setChecking] = useState(false);
  const queryClient = useQueryClient();
  const apply = useApplySync(collection);
  const applyingAnywhere = useIsApplying(collection);
  const { last, dismiss } = useLastApply(collection);

  // The first entry is selected by default, and again when the chosen one is gone.
  const planData = plan.data;
  const sel = useSyncSelection(planData, links, setLinks);
  const { setMany, setManyVariables, toggle, toggleVariable, approved, variables } = sel;

  // Select all covers every match of the search, rows past the cap included.
  const matchedKeys = useMemo(
    () => (planData ? filterItems(planData.items, query).map((i) => i.key) : []),
    [planData, query],
  );
  const matchedVariables = useMemo(
    () => (planData ? filterVariables(planData.variableChanges, query).map((v) => v.name) : []),
    [planData, query],
  );
  const selectAll = useCallback(() => {
    setMany(matchedKeys, true);
    setManyVariables(matchedVariables, true);
  }, [setMany, setManyVariables, matchedKeys, matchedVariables]);
  const selectNone = useCallback(() => {
    setMany(Array.from(approved), false);
    setManyVariables(Array.from(variables), false);
  }, [setMany, setManyVariables, approved, variables]);
  const approvedItems = useMemo(
    () => (planData ? planData.items.filter((i) => approved.has(i.key)) : []),
    [planData, approved],
  );

  // An emptied selection, for example after a plan change, closes the dialog for good.
  if (confirmOpen && approvedItems.length + variables.size === 0) setConfirmOpen(false);

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
    // Only the skips go out, with no links. Other approvals wait for the Apply button.
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
    planData && findEntry(planData, selectedId)
      ? selectedId
      : planData
        ? firstEntryId(planData)
        : null;

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
  if (!s) return null;

  const noChanges = (
    <EmptyState
      cause='none-yet'
      size='compact'
      title='No changes found'
      reason={
        s.consent !== 'granted'
          ? 'Approve the spec link first, then check for changes.'
          : s.lastCheckedAt
            ? `Last checked ${formatRelativeTime(s.lastCheckedAt)}.`
            : 'The spec has not been checked yet.'
      }
    />
  );

  let body: ReactNode;
  if (!planEnabled) {
    body = noChanges;
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
    body = noChanges;
  } else {
    body = (
      <div className='flex min-h-0 flex-1 flex-col'>
        {s.consent !== 'granted' && (
          <p className='px-4 pt-2 text-xs text-muted-foreground'>
            This pending spec came with the collection files. It may not have been checked on this
            machine.
          </p>
        )}
        <OpenApiSyncOverview plan={planData} status={s} />
        <OpenApiSyncActionsBar
          query={query}
          onQueryChange={setQuery}
          filteredCount={matchedKeys.length + matchedVariables.length}
          selectedCount={approved.size + variables.size}
          itemSelectedCount={approvedItems.length}
          locked={plan.isPlaceholderData}
          busy={apply.isPending || applyingAnywhere || plan.isFetching}
          onSelectAll={selectAll}
          onSelectNone={selectNone}
          onSkip={skipSelected}
          onApply={() => setConfirmOpen(true)}
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
              plan={planData}
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
        <ApplyConfirmDialog
          open={confirmOpen && approvedItems.length + variables.size > 0}
          onOpenChange={setConfirmOpen}
          items={approvedItems}
          variableCount={variables.size}
          onConfirm={confirmApply}
        />
      </div>
    );
  }

  return (
    <div className='flex h-full flex-col overflow-hidden'>
      <OpenApiSyncHeader collection={collection} status={s} />
      {s.consent !== 'granted' && (
        <OpenApiSyncConsentBanner
          key={collection}
          collection={collection}
          location={s.location}
          consent={s.consent}
        />
      )}
      <div className='flex min-h-0 flex-1 flex-col'>{body}</div>
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
