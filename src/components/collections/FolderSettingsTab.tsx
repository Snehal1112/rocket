import { FolderCog } from 'lucide-react';
import { useCallback, useEffect } from 'react';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { useFolderSettings } from '@/hooks/useFolderSettings';
import type { FolderSettings } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { FolderTab } from '@/types/pane-types';
import { AuthSection } from './folder-settings/AuthSection';
import { DocsSection } from './folder-settings/DocsSection';
import { FolderSaveButton } from './folder-settings/FolderSaveButton';
import { HeadersSection } from './folder-settings/HeadersSection';
import { ScriptSection } from './folder-settings/ScriptSection';
import { FOLDER_SECTIONS, isFolderSection } from './folder-settings/sections';
import { TestSection } from './folder-settings/TestSection';
import { VarsSection } from './folder-settings/VarsSection';

interface FolderSettingsTabProps {
  tab: FolderTab;
}

// Folder settings shell: a header and one sub-tab per section. The sections fill in over plans 09 to 11.
export function FolderSettingsTab({ tab }: FolderSettingsTabProps) {
  const updateFolderSection = usePaneStore((s) => s.updateFolderSection);
  const folderName = tab.folderPath.split('/').pop() ?? tab.folderPath;
  const breadcrumb = [tab.collectionName, ...tab.folderPath.split('/')].join(' / ');
  const { settings, setSettings, isDirty, isLoaded, error, save, saveState } = useFolderSettings(
    tab.collectionName,
    tab.folderPath,
  );
  const markDirty = usePaneStore((s) => s.markDirty);
  const markClean = usePaneStore((s) => s.markClean);

  // Sections send partial patches. Merge them into the whole settings object.
  const handleChange = useCallback(
    (patch: Partial<FolderSettings>) => setSettings((prev) => ({ ...prev, ...patch })),
    [setSettings],
  );

  // Show the unsaved marker on the tab strip.
  useEffect(() => {
    if (isDirty) markDirty(tab.id);
    else markClean(tab.id);
  }, [isDirty, tab.id, markDirty, markClean]);

  // Cmd/Ctrl+S is dispatched by useKeyboardShortcuts for the active tab.
  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent<{ tabId: string }>).detail;
      if (detail?.tabId !== tab.id) return;
      void save();
    };
    window.addEventListener('rocket:save-draft', handler);
    return () => window.removeEventListener('rocket:save-draft', handler);
  }, [tab.id, save]);

  const section = {
    collectionName: tab.collectionName,
    folderPath: tab.folderPath,
    settings,
    onChange: handleChange,
  };

  return (
    <div className='flex h-full min-h-0 flex-col'>
      <div className='flex shrink-0 items-center gap-2 border-b border-border px-4 py-3'>
        <FolderCog aria-hidden='true' className='h-5 w-5 shrink-0 text-muted-foreground' />
        <div className='min-w-0'>
          <h1 className='truncate text-base font-semibold text-foreground'>{folderName}</h1>
          <p className='truncate text-xs text-muted-foreground'>{breadcrumb}</p>
        </div>
        <div className='ml-auto'>
          <FolderSaveButton
            isDirty={isDirty}
            isLoaded={isLoaded}
            saveState={saveState}
            onSave={() => void save()}
          />
        </div>
      </div>
      {error ? (
        <div className='flex flex-1 items-center justify-center text-sm text-destructive'>
          {error}
        </div>
      ) : !isLoaded ? (
        <div className='flex flex-1 items-center justify-center text-sm text-muted-foreground'>
          Loading...
        </div>
      ) : (
        <Tabs
          value={tab.activeSection}
          onValueChange={(value) => {
            if (isFolderSection(value)) updateFolderSection(tab.id, value);
          }}
          className='flex min-h-0 flex-1 flex-col px-4 pt-3'
        >
          <TabsList className='self-start'>
            {FOLDER_SECTIONS.map((s) => (
              <TabsTrigger key={s.id} value={s.id}>
                {s.label}
              </TabsTrigger>
            ))}
          </TabsList>
          <TabsContent value='headers' className='min-h-0 flex-1 overflow-auto'>
            <HeadersSection {...section} />
          </TabsContent>
          <TabsContent value='script' className='min-h-0 flex-1 overflow-auto'>
            <ScriptSection {...section} />
          </TabsContent>
          <TabsContent value='test' className='min-h-0 flex-1 overflow-auto'>
            <TestSection {...section} />
          </TabsContent>
          <TabsContent value='vars' className='min-h-0 flex-1 overflow-auto'>
            <VarsSection {...section} />
          </TabsContent>
          <TabsContent value='auth' className='min-h-0 flex-1 overflow-auto'>
            <AuthSection {...section} />
          </TabsContent>
          <TabsContent value='docs' className='min-h-0 flex-1 overflow-auto'>
            <DocsSection {...section} />
          </TabsContent>
        </Tabs>
      )}
    </div>
  );
}
