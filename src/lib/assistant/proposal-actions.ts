import { collectAllTabs, isPathWithin, mapApiRequestToState } from '@/lib/pane-utils';
import { collectionKeys } from '@/lib/queries/collection-queries';
import { getQueryClient } from '@/lib/query-client';
import {
  type AgentProposal,
  acceptAgentProposal,
  getRequest,
  rejectAgentProposal,
} from '@/lib/tauri-api';
import { useAssistantStore } from '@/stores/assistant-store';
import { type PaneState, usePaneStore } from '@/stores/pane-store';
import { isRequestTab, type RequestTab } from '@/types/pane-types';
import { proposalTarget } from './proposal-view';

type Change = AgentProposal['change'];
type TabSource = Pick<PaneState, 'root' | 'collectionTabState'>;

/**
 * Request tabs, live or parked after a collection switch, that show an item
 * this change rewrites, moves or renames. Creates and env vars touch none.
 */
export function findAffectedRequestTabs(state: TabSource, change: Change): RequestTab[] {
  if (change.op === 'createFolder' || change.op === 'createRequest') return [];
  if (change.op === 'setEnvVar') return [];
  const target = proposalTarget(change);
  const targetPath = target.path;
  if (!targetPath) return [];
  const tabs = [
    ...collectAllTabs(state.root),
    ...Object.values(state.collectionTabState).flatMap((entry) => entry.tabs),
  ];
  return tabs.filter(
    (tab): tab is RequestTab =>
      isRequestTab(tab) &&
      tab.source?.collection === target.collection &&
      isPathWithin(tab.source.path, targetPath),
  );
}

/** A dirty tab would later save its stale copy over the accepted change. */
export function hasDirtyAffectedTab(state: TabSource, change: Change): boolean {
  return findAffectedRequestTabs(state, change).some((tab) => tab.isDirty);
}

// Shows the accepted version in clean HTTP tabs of the changed request. Tabs
// with edits are never touched, and Accept is disabled while one exists.
async function refreshCleanOpenTabs(change: Change): Promise<void> {
  if (change.op !== 'updateRequest' && change.op !== 'editScript') return;
  const tabs = collectAllTabs(usePaneStore.getState().root).filter(
    (tab): tab is RequestTab =>
      isRequestTab(tab) &&
      tab.tabType === 'request' &&
      !tab.isDirty &&
      tab.request.requestType === 'http' &&
      tab.source?.collection === change.collection &&
      tab.source.path === change.requestPath,
  );
  if (tabs.length === 0) return;
  try {
    const fresh = mapApiRequestToState(
      await getRequest(change.collection, change.requestPath),
      true,
    );
    for (const tab of tabs) {
      usePaneStore.getState().updateRequest(tab.id, fresh);
      usePaneStore.getState().markClean(tab.id);
    }
  } catch (err) {
    console.error('[assistant] failed to refresh an open tab', err);
  }
}

export async function acceptProposal(proposal: AgentProposal): Promise<void> {
  const result = await acceptAgentProposal(proposal.sessionId, proposal.id);
  useAssistantStore.getState().upsertProposal(result);
  if (result.status !== 'accepted') return;
  void getQueryClient().invalidateQueries({ queryKey: collectionKeys.all });
  await refreshCleanOpenTabs(proposal.change);
}

export async function rejectProposal(proposal: AgentProposal): Promise<void> {
  const result = await rejectAgentProposal(proposal.sessionId, proposal.id);
  useAssistantStore.getState().upsertProposal(result);
}
