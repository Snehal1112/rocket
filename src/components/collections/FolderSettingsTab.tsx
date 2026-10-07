import { FolderCog } from 'lucide-react';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { usePaneStore } from '@/stores/pane-store';
import type { FolderTab } from '@/types/pane-types';
import { AuthSection } from './folder-settings/AuthSection';
import { DocsSection } from './folder-settings/DocsSection';
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
  const section = { collectionName: tab.collectionName, folderPath: tab.folderPath };

  return (
    <div className='flex h-full min-h-0 flex-col'>
      <div className='flex shrink-0 items-center gap-2 border-b border-border px-4 py-3'>
        <FolderCog aria-hidden='true' className='h-5 w-5 shrink-0 text-muted-foreground' />
        <div className='min-w-0'>
          <h1 className='truncate text-base font-semibold text-foreground'>{folderName}</h1>
          <p className='truncate text-xs text-muted-foreground'>{breadcrumb}</p>
        </div>
      </div>
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
    </div>
  );
}
