import { open } from '@tauri-apps/plugin-dialog';
import { FileUp, Plus, Type, X } from 'lucide-react';
import { useCallback, useState } from 'react';
import { SingleLineEditor } from '@/components/editor';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import type { VariableScopeEntry, VariableSource } from '@/lib/url-variables';
import { OUTSIDE_WORKSPACE_MESSAGE, toUploadFilePath } from '@/lib/workspace-file-path';
import type { KeyValueEntry } from '@/types/pane-types';

interface FormDataEditorProps {
  entries: KeyValueEntry[];
  onChange: (entries: KeyValueEntry[]) => void;
  variableContext?: Map<string, VariableScopeEntry>;
  /** Workspace folder. When set, picked files must be inside it. */
  workspacePath?: string;
  /** Collection of the request. A picked file inside its folder is stored relative to it. */
  collection?: string;
  onNavigateToSource?: (source: VariableSource | 'pathParam', key: string) => void;
}

// Multipart rows: each is text or a file, with an optional Content-Type for the part.
export function FormDataEditor({
  entries,
  onChange,
  variableContext,
  workspacePath,
  collection,
  onNavigateToSource,
}: FormDataEditorProps) {
  const [rejectedRowId, setRejectedRowId] = useState<string | null>(null);
  const updateEntry = useCallback(
    (id: string, patch: Partial<KeyValueEntry>) =>
      onChange(entries.map((e) => (e.id === id ? { ...e, ...patch } : e))),
    [entries, onChange],
  );

  const pickFile = useCallback(
    async (id: string) => {
      // The picker is disabled without a workspace, so this is only a guard.
      if (!workspacePath) return;
      const result = await open({ multiple: false, title: 'Select file for form field' });
      if (typeof result !== 'string') return;
      // The executor only reads files inside the workspace.
      const relative = toUploadFilePath(result, workspacePath, collection);
      if (relative === null) {
        setRejectedRowId(id);
        return;
      }
      setRejectedRowId(null);
      updateEntry(id, { value: relative });
    },
    [updateEntry, workspacePath, collection],
  );

  const addEntry = useCallback(
    () => onChange([...entries, { id: crypto.randomUUID(), key: '', value: '', enabled: true }]),
    [entries, onChange],
  );

  return (
    <div className='space-y-2'>
      {entries.map((entry, idx) => {
        const row = idx + 1;
        const isFile = entry.entryType === 'file';
        return (
          <div key={entry.id} className='flex items-center gap-2'>
            <Checkbox
              checked={entry.enabled}
              onCheckedChange={(checked) => updateEntry(entry.id, { enabled: !!checked })}
              aria-label={`${entry.enabled ? 'Disable' : 'Enable'} ${entry.key || 'unnamed'}`}
            />
            <Input
              aria-label={`Key for row ${row}`}
              placeholder='Field name'
              value={entry.key}
              onChange={(e) => updateEntry(entry.id, { key: e.target.value })}
              className='min-w-0 flex-1 font-mono text-xs'
            />
            <Button
              variant='outline'
              size='sm'
              className='h-8 w-16 shrink-0 text-xs'
              aria-label={`Field type for row ${row}: ${isFile ? 'File' : 'Text'}`}
              onClick={() =>
                updateEntry(entry.id, { entryType: isFile ? 'text' : 'file', value: '' })
              }
            >
              {isFile ? (
                <FileUp className='mr-1 h-3 w-3' aria-hidden='true' />
              ) : (
                <Type className='mr-1 h-3 w-3' aria-hidden='true' />
              )}
              {isFile ? 'File' : 'Text'}
            </Button>
            <div className='min-w-0 flex-1'>
              {isFile ? (
                <>
                  <Button
                    variant='outline'
                    size='sm'
                    className='h-8 w-full justify-start truncate text-xs'
                    aria-label={`Choose file for row ${row}`}
                    disabled={!workspacePath}
                    title={workspacePath ? undefined : 'Waiting for the workspace to load'}
                    onClick={() => pickFile(entry.id)}
                  >
                    {entry.value
                      ? (entry.value.split(/[\\/]/).pop() ?? entry.value)
                      : 'Choose file'}
                  </Button>
                  {rejectedRowId === entry.id && (
                    <p role='alert' className='mt-1 text-xs text-destructive'>
                      {OUTSIDE_WORKSPACE_MESSAGE}
                    </p>
                  )}
                </>
              ) : (
                <SingleLineEditor
                  placeholder='Value'
                  value={entry.value}
                  onChange={(next) => updateEntry(entry.id, { value: next })}
                  className='text-xs'
                  variableContext={variableContext}
                  onNavigateToSource={onNavigateToSource}
                />
              )}
            </div>
            <Input
              aria-label={`Content type for row ${row}`}
              placeholder='auto'
              value={entry.contentType ?? ''}
              onChange={(e) => updateEntry(entry.id, { contentType: e.target.value })}
              className='w-36 shrink-0 font-mono text-xs'
            />
            <Button
              variant='ghost'
              size='icon'
              className='h-7 w-7'
              aria-label={`Remove ${entry.key || 'unnamed'}`}
              onClick={() => onChange(entries.filter((e) => e.id !== entry.id))}
            >
              <X className='h-3.5 w-3.5' />
            </Button>
          </div>
        );
      })}
      <Button variant='ghost' size='sm' onClick={addEntry} className='text-xs'>
        <Plus className='mr-1 h-3.5 w-3.5' />
        Add Field
      </Button>
    </div>
  );
}
