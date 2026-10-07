import { useEffect, useState } from 'react';
import { MarkdownEditor } from '@/components/collections/MarkdownEditor';
import type { FolderSectionProps } from './sections';

/** Markdown documentation for the folder, stored as the top-level `docs` of folder.yml. */
export function DocsSection({
  collectionName,
  folderPath,
  settings,
  onChange,
}: FolderSectionProps) {
  const [mode, setMode] = useState<'edit' | 'preview'>('preview');

  // Each folder starts in preview mode, like the collection Documentation tab.
  // biome-ignore lint/correctness/useExhaustiveDependencies: the dependencies are the reset triggers.
  useEffect(() => {
    setMode('preview');
  }, [collectionName, folderPath]);

  return (
    <div className='flex h-full min-h-0 flex-col overflow-hidden p-6'>
      <MarkdownEditor
        value={settings.docs ?? ''}
        onChange={(value) => onChange({ docs: value === '' ? undefined : value })}
        mode={mode}
        onModeChange={setMode}
      />
    </div>
  );
}
