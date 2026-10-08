import { CollectionVariablesEditor } from '@/components/collections/CollectionVariablesEditor';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { ScrollArea } from '@/components/ui/scroll-area';
import type { CollectionVariable } from '@/lib/tauri-api';
import type { FolderSectionProps } from './sections';

/** Pre-request variables for every request in this folder and its sub-folders. */
export function VarsSection({ settings, onChange }: FolderSectionProps) {
  const handleChange = (variables: CollectionVariable[]) => onChange({ variables });

  return (
    <ScrollArea className='h-full'>
      <div className='p-6 max-w-3xl mx-auto'>
        <Card>
          <CardHeader className='pb-3 pt-4 px-4 border-b border-border/40'>
            <CardTitle className='text-sm font-medium'>Pre Request</CardTitle>
            <p className='text-xs text-muted-foreground'>
              These variables are resolved before each request in this folder and its sub-folders
              runs. A request's own variables win over them, and they win over environment and
              collection variables. To set a variable after a response, use a script.
            </p>
          </CardHeader>
          <CardContent className='p-4'>
            <CollectionVariablesEditor
              variables={settings.variables}
              onChange={handleChange}
              showDescription={false}
            />
          </CardContent>
        </Card>
      </div>
    </ScrollArea>
  );
}
