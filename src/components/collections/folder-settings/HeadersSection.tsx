import { useEffect, useState } from 'react';
import { HeadersEditor } from '@/components/request/HeadersEditor';
import { useFolderVariableContext } from '@/hooks/useFolderVariableContext';
import { entriesToHeaders, headersToEntries } from '@/lib/folder-settings-convert';
import type { KeyValueEntry } from '@/types/pane-types';
import type { FolderSectionProps } from './sections';

export function HeadersSection({
  collectionName,
  folderPath,
  settings,
  onChange,
}: FolderSectionProps) {
  // Rows live here, not in `settings`, so a row with no name yet is not dropped on the
  // next render. Only complete rows are pushed up.
  const [entries, setEntries] = useState<KeyValueEntry[]>(() => headersToEntries(settings.headers));
  const { variableContext } = useFolderVariableContext(
    collectionName,
    folderPath,
    settings.variables,
  );

  // biome-ignore lint/correctness/useExhaustiveDependencies: only an outside change to settings.headers resets the rows; the local rows are compared inside.
  useEffect(() => {
    if (
      JSON.stringify(entriesToHeaders(entries, settings.headers)) !==
      JSON.stringify(settings.headers)
    ) {
      setEntries(headersToEntries(settings.headers));
    }
  }, [settings.headers]);

  const handleChange = (next: KeyValueEntry[]) => {
    setEntries(next);
    const headers = entriesToHeaders(next, settings.headers);
    if (JSON.stringify(headers) === JSON.stringify(settings.headers)) return;
    onChange({ headers });
  };

  return (
    <div className='p-4 max-w-3xl'>
      <p className='mb-3 text-xs text-muted-foreground'>
        These headers are sent with every request in this folder and its subfolders. A request, or a
        folder closer to it, replaces a header with the same name.
      </p>
      <HeadersEditor headers={entries} onChange={handleChange} variableContext={variableContext} />
    </div>
  );
}
