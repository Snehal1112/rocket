import type { FolderTab } from '@/types/pane-types';

// Stub so the tab router compiles. Task 2 of plan 08 replaces it with the real shell.
export function FolderSettingsTab({ tab }: { tab: FolderTab }) {
  return <div className='p-4 text-sm text-muted-foreground'>{tab.folderPath}</div>;
}
