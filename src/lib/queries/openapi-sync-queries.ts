import { type QueryClient, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useCallback, useEffect, useRef, useSyncExternalStore } from 'react';
import {
  applyOpenApiSync,
  checkOpenApiSyncNow,
  clearOpenApiSyncToken,
  discardOpenApiSyncBackup,
  disconnectOpenApiSync,
  getOpenApiSyncBadges,
  getOpenApiSyncChanges,
  getOpenApiSyncOverview,
  getOpenApiSyncPlan,
  getOpenApiSyncSpecView,
  getOpenApiSyncStatus,
  grantOpenApiSyncConsent,
  type OpenApiApplyReport,
  type OpenApiSelection,
  type OpenApiSyncInterval,
  onOpenApiSyncApplied,
  onOpenApiSyncChecked,
  resetOpenApiSync,
  restoreOpenApiSync,
  revokeOpenApiSyncConsent,
  setOpenApiSyncInterval,
  setOpenApiSyncToken,
} from '@/lib/tauri-api';
import { collectionKeys } from './collection-queries';

/** The payload of an `openapi-sync-checked` event. */
export interface OpenApiCheckedEvent {
  collection: string;
  status: string;
}

type Links = Array<[string, string]>;

const NO_LINKS: Links = [];

export const openApiSyncKeys = {
  all: ['openapi-sync'] as const,
  /** Matches every status query of the collection, whatever `includeToken` was. */
  statusAll: (collection: string) => ['openapi-sync', 'status', collection] as const,
  /** Matches every plan query of the collection, whatever the links were. */
  planAll: (collection: string) => ['openapi-sync', 'plan', collection] as const,
  overview: (collection: string) => ['openapi-sync', 'overview', collection] as const,
  changes: (collection: string) => ['openapi-sync', 'changes', collection] as const,
  specView: (collection: string) => ['openapi-sync', 'spec-view', collection] as const,
  /** The waiting counts of every collection, so it is not scoped to one. */
  badges: ['openapi-sync', 'badges'] as const,
  status: (collection: string, includeToken = false) =>
    ['openapi-sync', 'status', collection, { includeToken }] as const,
  plan: (collection: string, links: Links = NO_LINKS) =>
    ['openapi-sync', 'plan', collection, { links }] as const,
};

/** The sync status of a collection, or null when it has no spec source. */
export function useOpenApiSyncStatus(
  collection: string | null | undefined,
  includeToken = false,
  enabled = true,
) {
  return useQuery({
    // Local IPC work must not pause when WebKit reports the machine as offline.
    networkMode: 'always',
    queryKey: openApiSyncKeys.status(collection ?? '', includeToken),
    queryFn: () => getOpenApiSyncStatus(collection ?? '', includeToken),
    enabled: enabled && !!collection,
    // A background check changes the answer, so it is never trusted for long.
    staleTime: 0,
    // An unreadable state file is a result to show, not a hiccup to retry.
    retry: false,
  });
}

/** The plan of the pending spec. It only runs while enabled, and a new link set refetches. */
export function useOpenApiSyncPlan(
  collection: string | null | undefined,
  enabled: boolean,
  links: Links = NO_LINKS,
) {
  return useQuery({
    // Local IPC work must not pause when WebKit reports the machine as offline.
    networkMode: 'always',
    queryKey: openApiSyncKeys.plan(collection ?? '', links),
    queryFn: () => getOpenApiSyncPlan(collection ?? '', links),
    // The old plan stays on screen while a new link set loads, for the same collection only.
    placeholderData: (previous, previousQuery) =>
      previousQuery?.queryKey[2] === (collection ?? '') ? previous : undefined,
    enabled: enabled && !!collection,
    staleTime: 0,
    retry: false,
  });
}

/** The overview numbers of a collection. */
export function useOpenApiSyncOverview(collection: string | null | undefined) {
  return useQuery({
    // Local IPC work must not pause when WebKit reports the machine as offline.
    networkMode: 'always',
    queryKey: openApiSyncKeys.overview(collection ?? ''),
    queryFn: () => getOpenApiSyncOverview(collection ?? ''),
    enabled: !!collection,
    staleTime: 0,
    retry: false,
  });
}

/** The modified, added and deleted requests of a collection against its baseline. */
export function useOpenApiSyncChanges(collection: string | null | undefined) {
  return useQuery({
    networkMode: 'always',
    queryKey: openApiSyncKeys.changes(collection ?? ''),
    queryFn: () => getOpenApiSyncChanges(collection ?? ''),
    enabled: !!collection,
    staleTime: 0,
    retry: false,
  });
}

/** The latest stored spec of a collection, for the read-only viewer. */
export function useOpenApiSyncSpecView(collection: string | null | undefined) {
  return useQuery({
    networkMode: 'always',
    queryKey: openApiSyncKeys.specView(collection ?? ''),
    queryFn: () => getOpenApiSyncSpecView(collection ?? ''),
    enabled: !!collection,
    staleTime: 0,
    retry: false,
  });
}

/** How many updates wait in each collection that has a spec source. */
export function useOpenApiSyncBadges() {
  return useQuery({
    networkMode: 'always',
    queryKey: openApiSyncKeys.badges,
    queryFn: getOpenApiSyncBadges,
    staleTime: 0,
    retry: false,
  });
}

function invalidateSync(queryClient: QueryClient, collection: string): void {
  void queryClient.invalidateQueries({ queryKey: openApiSyncKeys.overview(collection) });
  void queryClient.invalidateQueries({ queryKey: openApiSyncKeys.changes(collection) });
  void queryClient.invalidateQueries({ queryKey: openApiSyncKeys.specView(collection) });
  void queryClient.invalidateQueries({ queryKey: openApiSyncKeys.badges });
  void queryClient.invalidateQueries({ queryKey: openApiSyncKeys.statusAll(collection) });
  void queryClient.invalidateQueries({ queryKey: openApiSyncKeys.planAll(collection) });
}

/** Runs a check now and refreshes the status and plan of the collection. */
export function useCheckNow(collection: string) {
  const queryClient = useQueryClient();
  return useMutation({
    networkMode: 'always',
    mutationFn: () => checkOpenApiSyncNow(collection),
    onSettled: () => invalidateSync(queryClient, collection),
  });
}

export function useSetInterval(collection: string) {
  const queryClient = useQueryClient();
  return useMutation({
    networkMode: 'always',
    mutationFn: (interval: OpenApiSyncInterval) => setOpenApiSyncInterval(collection, interval),
    onSettled: () => invalidateSync(queryClient, collection),
  });
}

export function useSetToken(collection: string) {
  const queryClient = useQueryClient();
  return useMutation({
    networkMode: 'always',
    mutationFn: (token: string) => setOpenApiSyncToken(collection, token),
    // The cache drops the mutation as soon as it settles, so the token does not linger.
    gcTime: 0,
    onSettled: () => invalidateSync(queryClient, collection),
  });
}

export function useClearToken(collection: string) {
  const queryClient = useQueryClient();
  return useMutation({
    networkMode: 'always',
    mutationFn: () => clearOpenApiSyncToken(collection),
    onSettled: () => invalidateSync(queryClient, collection),
  });
}

export function useGrantConsent(collection: string) {
  const queryClient = useQueryClient();
  return useMutation({
    networkMode: 'always',
    mutationFn: (location: string) => grantOpenApiSyncConsent(collection, location),
    onSettled: () => invalidateSync(queryClient, collection),
  });
}

export function useRevokeConsent(collection: string) {
  const queryClient = useQueryClient();
  return useMutation({
    networkMode: 'always',
    mutationFn: () => revokeOpenApiSyncConsent(collection),
    onSettled: () => invalidateSync(queryClient, collection),
  });
}

/** Removes the sync state of a collection and refreshes everything that showed it. */
/** After a reset, restore or discard the requests on disk changed, so the tree reloads too. */
function refreshAfterReset(queryClient: QueryClient, collection: string): void {
  invalidateSync(queryClient, collection);
  void queryClient.invalidateQueries({ queryKey: collectionKeys.all });
}

/** Puts the spec-owned fields of the given operations back to the last synced spec. */
export function useResetOpenApiSync(collection: string) {
  const queryClient = useQueryClient();
  return useMutation({
    networkMode: 'always',
    mutationFn: (keys: string[]) => resetOpenApiSync(collection, keys),
    onSettled: () => refreshAfterReset(queryClient, collection),
  });
}

/** Writes the backed-up version of a reset request back. */
export function useRestoreOpenApiSync(collection: string) {
  const queryClient = useQueryClient();
  return useMutation({
    networkMode: 'always',
    mutationFn: (key: string) => restoreOpenApiSync(collection, key),
    onSettled: () => refreshAfterReset(queryClient, collection),
  });
}

/** Drops a backup without touching any request. */
export function useDiscardOpenApiBackup(collection: string) {
  const queryClient = useQueryClient();
  return useMutation({
    networkMode: 'always',
    mutationFn: (key: string) => discardOpenApiSyncBackup(collection, key),
    onSettled: () => refreshAfterReset(queryClient, collection),
  });
}

export function useDisconnectSync(collection: string) {
  const queryClient = useQueryClient();
  return useMutation({
    networkMode: 'always',
    mutationFn: (removeToken?: boolean) => disconnectOpenApiSync(collection, removeToken),
    onSuccess: () => {
      setLastApply(collection, null);
    },
    onSettled: () => {
      invalidateSync(queryClient, collection);
      void queryClient.invalidateQueries({ queryKey: collectionKeys.all });
    },
  });
}

// One apply per collection at a time, shared by every component and surviving remounts.
const applyingCollections = new Set<string>();
const applyListeners = new Set<() => void>();

function setApplying(collection: string, on: boolean): void {
  if (on) applyingCollections.add(collection);
  else applyingCollections.delete(collection);
  for (const l of applyListeners) l();
}

function subscribeApplying(listener: () => void): () => void {
  applyListeners.add(listener);
  return () => applyListeners.delete(listener);
}

/** True while an apply runs for the collection, in any component. */
export function isApplying(collection: string): boolean {
  return applyingCollections.has(collection);
}

/** Re-renders when an apply starts or ends for the collection. */
export function useIsApplying(collection: string): boolean {
  return useSyncExternalStore(subscribeApplying, () => isApplying(collection));
}

/** Forgets the in-flight and last-outcome state of every collection, after a workspace switch. */
export function clearApplyState(): void {
  applyingCollections.clear();
  for (const l of applyListeners) l();
  lastApplies.clear();
  for (const l of lastApplyListeners) l();
}

/** What the last apply of a collection ended with: a report or an error. */
export interface LastApply {
  report?: OpenApiApplyReport;
  error?: unknown;
  /** How many variable changes the apply carried. */
  variableCount: number;
}

// The outcome lives here, not in a component, so leaving the tab mid-apply does not lose it.
const lastApplies = new Map<string, LastApply>();
const lastApplyListeners = new Set<() => void>();

function setLastApply(collection: string, value: LastApply | null): void {
  if (value) lastApplies.set(collection, value);
  else lastApplies.delete(collection);
  for (const l of lastApplyListeners) l();
}

function subscribeLastApply(listener: () => void): () => void {
  lastApplyListeners.add(listener);
  return () => lastApplyListeners.delete(listener);
}

/** The last apply outcome of the collection, and a way to dismiss it. */
export function useLastApply(collection: string): { last: LastApply | null; dismiss: () => void } {
  const last = useSyncExternalStore(subscribeLastApply, () => lastApplies.get(collection) ?? null);
  const dismiss = useCallback(() => setLastApply(collection, null), [collection]);
  return { last, dismiss };
}

/**
 * Applies a selection and refreshes status, plan and the collection tree. Only one apply
 * runs at a time per collection, a call while one is in flight is a silent no-op
 * for `mutate` and a rejection for `mutateAsync`.
 */
export function useApplySync(collection: string) {
  const queryClient = useQueryClient();
  const mutation = useMutation<OpenApiApplyReport, unknown, OpenApiSelection>({
    networkMode: 'always',
    mutationFn: async (selection) => {
      try {
        const report = await applyOpenApiSync(collection, selection);
        setLastApply(collection, { report, variableCount: selection.variables.length });
        return report;
      } catch (error) {
        setLastApply(collection, { error, variableCount: 0 });
        throw error;
      } finally {
        setApplying(collection, false);
      }
    },
    onSettled: () => {
      invalidateSync(queryClient, collection);
      void queryClient.invalidateQueries({ queryKey: collectionKeys.all });
    },
  });
  const { mutate, mutateAsync } = mutation;
  const guardedMutate = useCallback<typeof mutate>(
    (selection, options) => {
      if (applyingCollections.has(collection)) return;
      setApplying(collection, true);
      setLastApply(collection, null);
      mutate(selection, options);
    },
    [collection, mutate],
  );
  const guardedMutateAsync = useCallback<typeof mutateAsync>(
    (selection, options) => {
      if (applyingCollections.has(collection)) {
        return Promise.reject(new Error('An apply is already running.'));
      }
      setApplying(collection, true);
      setLastApply(collection, null);
      return mutateAsync(selection, options);
    },
    [collection, mutateAsync],
  );
  return { ...mutation, mutate: guardedMutate, mutateAsync: guardedMutateAsync };
}

/**
 * Refreshes the status and plan queries of the collection named in a sync check event, and
 * also the collection tree for an apply event. Mount it once, through `useOpenApiSyncGlobalEvents`.
 */
export function useOpenApiSyncEvents(onChecked?: (event: OpenApiCheckedEvent) => void): void {
  const queryClient = useQueryClient();
  const checkedRef = useRef(onChecked);
  checkedRef.current = onChecked;
  useEffect(() => {
    const unsubChecked = onOpenApiSyncChecked((event) => {
      invalidateSync(queryClient, event.collection);
      checkedRef.current?.(event);
    });
    const unsubApplied = onOpenApiSyncApplied(({ collection }) => {
      invalidateSync(queryClient, collection);
      void queryClient.invalidateQueries({ queryKey: collectionKeys.all });
    });
    return () => {
      void unsubChecked.then((fn) => fn()).catch(() => undefined);
      void unsubApplied.then((fn) => fn()).catch(() => undefined);
    };
  }, [queryClient]);
}

/**
 * The one app-wide pair of sync listeners. Every event also refreshes the waiting badges.
 * Mount it once from `OpenApiSyncNotifier`, not from the sync tab.
 */
export function useOpenApiSyncGlobalEvents(onChecked?: (event: OpenApiCheckedEvent) => void): void {
  useOpenApiSyncEvents(onChecked);
}
