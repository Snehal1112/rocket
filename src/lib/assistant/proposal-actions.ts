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
import { isRequestTab, type RequestTab, type Tab } from '@/types/pane-types';
import { proposalTarget } from './proposal-view';

type Change = AgentProposal['change'];
type TabSource = Pick<PaneState, 'root' | 'collectionTabState'>;

// Request tabs among `tabs` that show an item this change rewrites, moves or
// renames. Creates and env vars touch none.
function affectedAmong(tabs: Tab[], change: Change): RequestTab[] {
  if (change.op === 'createFolder' || change.op === 'createRequest') return [];
  if (change.op === 'setEnvVar') return [];
  const target = proposalTarget(change);
  const targetPath = target.path;
  if (!targetPath) return [];
  return tabs.filter(
    (tab): tab is RequestTab =>
      isRequestTab(tab) &&
      tab.source?.collection === target.collection &&
      isPathWithin(tab.source.path, targetPath),
  );
}

/** Request tabs, live or parked after a collection switch, that this change touches. */
export function findAffectedRequestTabs(state: TabSource, change: Change): RequestTab[] {
  return affectedAmong(
    [
      ...collectAllTabs(state.root),
      ...Object.values(state.collectionTabState).flatMap((entry) => entry.tabs),
    ],
    change,
  );
}

/** A dirty tab would later save its stale copy over the accepted change. */
export function hasDirtyAffectedTab(state: TabSource, change: Change): boolean {
  return findAffectedRequestTabs(state, change).some((tab) => tab.isDirty);
}

/**
 * Where a dirty tab of the changed item lives: 'open' for the live panes, or
 * 'parked:<collection>' for a tab parked after a collection switch. A string
 * keeps the result stable as a store selector.
 */
export function dirtyAffectedTabPlace(state: TabSource, change: Change): string | undefined {
  if (affectedAmong(collectAllTabs(state.root), change).some((tab) => tab.isDirty)) return 'open';
  for (const [key, entry] of Object.entries(state.collectionTabState)) {
    if (affectedAmong(entry.tabs, change).some((tab) => tab.isDirty)) return `parked:${key}`;
  }
  return undefined;
}

function lastSegment(path: string): string {
  return path.split('/').pop() ?? path;
}

function joinPath(parent: string, name: string): string {
  return parent ? `${parent}/${name}` : name;
}

function retarget(collection: string, oldPath: string, newPath: string, includeSelf = true): void {
  const store = usePaneStore.getState();
  store.retargetRequestTabs(collection, oldPath, newPath, includeSelf);
  store.renameScriptTabs(collection, oldPath, newPath);
  store.renameFolderTabs(collection, oldPath, newPath);
}

const DIRTY_WARNING =
  'A tab of this request was edited while the change was applied. Its edits were kept. Saving them, which also happens when the tab is switched or closed, will overwrite the accepted change.';

// Brings every open tab, live or parked, in line with an accepted change, so
// no stale copy can later autosave over it. Returns a warning for the card
// when a tab became dirty during the call and so kept its own edits.
async function syncTabsAfterAccept(change: Change): Promise<string | undefined> {
  const store = usePaneStore.getState();
  try {
    switch (change.op) {
      case 'updateRequest':
      case 'editScript': {
        const request = await getRequest(change.collection, change.requestPath);
        const { skippedDirty } = store.applyAcceptedRequest(
          change.collection,
          change.requestPath,
          mapApiRequestToState(request, true),
        );
        return skippedDirty > 0 ? DIRTY_WARNING : undefined;
      }
      case 'moveItem':
        retarget(
          change.collection,
          change.fromPath,
          joinPath(change.toFolder, lastSegment(change.fromPath)),
        );
        return undefined;
      case 'renameItem': {
        // A folder is renamed by moving it. A request keeps its file path and
        // only changes its name.
        const parent = change.path.includes('/')
          ? change.path.slice(0, change.path.lastIndexOf('/'))
          : '';
        retarget(change.collection, change.path, joinPath(parent, change.newName), false);
        const request = await getRequest(change.collection, change.path).catch(() => undefined);
        if (request) {
          // A tab with edits keeps them and only takes the new name, so its
          // next save does not write the old name back.
          store.applyAcceptedRequest(
            change.collection,
            change.path,
            mapApiRequestToState(request, true),
            request.name,
          );
        }
        return undefined;
      }
      default:
        return undefined;
    }
  } catch (err) {
    console.error('[assistant] failed to sync open tabs after accept', err);
    return undefined;
  }
}

/** Accepts a proposal. Resolves with a warning for the card, if any. */
export async function acceptProposal(proposal: AgentProposal): Promise<string | undefined> {
  const result = await acceptAgentProposal(proposal.sessionId, proposal.id);
  useAssistantStore.getState().upsertProposal(result);
  if (result.status !== 'accepted') return undefined;
  void getQueryClient().invalidateQueries({ queryKey: collectionKeys.all });
  return syncTabsAfterAccept(proposal.change);
}

export async function rejectProposal(proposal: AgentProposal): Promise<undefined> {
  const result = await rejectAgentProposal(proposal.sessionId, proposal.id);
  useAssistantStore.getState().upsertProposal(result);
  return undefined;
}
