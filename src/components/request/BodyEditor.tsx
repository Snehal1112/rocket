import { open } from '@tauri-apps/plugin-dialog';
import { FileUp } from 'lucide-react';
import { lazy, Suspense, useCallback, useState } from 'react';
import { EditorSkeleton } from '@/components/editor/EditorSkeleton';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { useActiveWorkspace } from '@/lib/queries/workspace-queries';
import type { VariableScopeEntry, VariableSource } from '@/lib/url-variables';
import { OUTSIDE_WORKSPACE_MESSAGE, toWorkspaceRelativePath } from '@/lib/workspace-file-path';
import type { BodyState, KeyValueEntry } from '@/types/pane-types';
import { FormDataEditor } from './FormDataEditor';
import { KeyValueEditor } from './KeyValueEditor';

// Lazy-load Monaco so it stays out of the initial JS bundle.
const MonacoWrapper = lazy(() =>
  import('@/components/editor/MonacoWrapper').then((m) => ({
    default: m.MonacoWrapper,
  })),
);

interface BodyEditorProps {
  body: BodyState;
  onChange: (body: BodyState) => void;
  variableContext?: Map<string, VariableScopeEntry>;
  onNavigateToSource?: (source: VariableSource | 'pathParam', key: string) => void;
}

export function BodyEditor({
  body,
  onChange,
  variableContext,
  onNavigateToSource,
}: BodyEditorProps) {
  const { data: activeWorkspace } = useActiveWorkspace();
  const workspacePath = activeWorkspace?.path;
  const [fileError, setFileError] = useState<string | null>(null);
  const setContent = useCallback(
    (content: string) => onChange({ ...body, content }),
    [body, onChange],
  );

  const setFormData = useCallback(
    (formData: KeyValueEntry[]) => onChange({ ...body, formData }),
    [body, onChange],
  );

  const handlePickFile = useCallback(async () => {
    // The picker is disabled without a workspace, so this is only a guard.
    if (!workspacePath) return;
    const result = await open({
      multiple: false,
      title: 'Select file for request body',
    });
    if (result) {
      const picked = result as string;
      // The executor only reads files inside the workspace, so store a relative path.
      const path = toWorkspaceRelativePath(picked, workspacePath);
      if (path === null) {
        setFileError(OUTSIDE_WORKSPACE_MESSAGE);
        return;
      }
      setFileError(null);
      onChange({
        ...body,
        filePath: path,
        fileName: path.split(/[\\/]/).pop() ?? 'unknown',
      });
    }
  }, [body, onChange, workspacePath]);

  const handleClear = useCallback(() => {
    onChange({ ...body, filePath: undefined, fileName: undefined });
  }, [body, onChange]);

  return (
    <div className='flex h-full flex-col space-y-2'>
      {/* Content area — fills remaining height. */}
      {body.mode === 'none' && (
        <div className='flex items-center justify-center h-32 text-muted-foreground text-sm'>
          No body content
        </div>
      )}

      {(body.mode === 'json' ||
        body.mode === 'xml' ||
        body.mode === 'text' ||
        body.mode === 'sparql') && (
        <div className='flex-1 border rounded-lg overflow-hidden min-h-[200px]'>
          <Suspense fallback={<EditorSkeleton />}>
            <MonacoWrapper
              value={body.content}
              onChange={(val) => setContent(val)}
              bodyMode={body.mode}
              height='100%'
              variableContext={variableContext}
            />
          </Suspense>
        </div>
      )}

      {body.mode === 'formdata' && (
        <FormDataEditor
          entries={body.formData}
          onChange={setFormData}
          variableContext={variableContext}
          workspacePath={workspacePath}
          onNavigateToSource={onNavigateToSource}
        />
      )}

      {body.mode === 'formurlencoded' && (
        <KeyValueEditor
          entries={body.formData}
          onChange={setFormData}
          keyPlaceholder='Field name'
          valuePlaceholder='Value'
          addLabel='Add Field'
          variableContext={variableContext}
          onNavigateToSource={onNavigateToSource}
        />
      )}

      {body.mode === 'binary' &&
        (body.filePath ? (
          <Card className='max-w-sm'>
            <CardContent className='flex items-center gap-3 p-4'>
              <FileUp className='size-5 shrink-0 text-muted-foreground' />
              <span className='flex-1 truncate text-sm'>{body.fileName}</span>
              <Button variant='ghost' size='sm' onClick={handleClear}>
                Clear
              </Button>
            </CardContent>
          </Card>
        ) : (
          <div className='space-y-1'>
            <Button variant='outline' onClick={handlePickFile} disabled={!workspacePath}>
              <FileUp className='mr-2 size-4' />
              Choose file
            </Button>
            {fileError && (
              <p role='alert' className='text-xs text-destructive'>
                {fileError}
              </p>
            )}
          </div>
        ))}
    </div>
  );
}
