import type { RunnerTab } from '@/types/pane-types';

// Names what a runner tab covers, for messages such as an empty request list.
export function runnerScopeLabel(tab: RunnerTab): string {
  if (tab.folderPath) return `The folder "${tab.folderPath}"`;
  if (tab.collectionName) return `The collection "${tab.collectionName}"`;
  return 'This selection';
}
