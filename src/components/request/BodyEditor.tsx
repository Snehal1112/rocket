import { useCallback, lazy, Suspense } from 'react';
import { FileUp } from 'lucide-react';
import { open } from '@tauri-apps/plugin-dialog';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import type { BodyState, KeyValueEntry } from '@/types/pane-types';
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
}

export function BodyEditor({ body, onChange }: BodyEditorProps) {
  const setContent = useCallback(
    (content: string) => onChange({ ...body, content }),
    [body, onChange],
  );

  const setFormData = useCallback(
    (formData: KeyValueEntry[]) => onChange({ ...body, formData }),
    [body, onChange],
  );

  const handlePickFile = useCallback(async () => {
    const result = await open({
      multiple: false,
      title: 'Select file for request body',
    });
    if (result) {
      const path = result as string;
      onChange({
        ...body,
        filePath: path,
        fileName: path.split('/').pop() ?? 'unknown',
      });
    }
  }, [body, onChange]);

  const handleClear = useCallback(() => {
    onChange({ ...body, filePath: undefined, fileName: undefined });
  }, [body, onChange]);

  return (
    <div className="flex h-full flex-col space-y-2">
      {/* Content area — fills remaining height. */}
      {body.mode === 'none' && (
        <div className="flex items-center justify-center h-32 text-muted-foreground text-sm">
          No body content
        </div>
      )}

      {(body.mode === 'json' || body.mode === 'xml' || body.mode === 'text') && (
        <div className="flex-1 border rounded min-h-[200px]">
          <Suspense
            fallback={
              <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
                Loading editor...
              </div>
            }
          >
            <MonacoWrapper
              value={body.content}
              onChange={(val) => setContent(val)}
              bodyMode={body.mode}
              height="100%"
            />
          </Suspense>
        </div>
      )}

      {body.mode === 'formdata' && (
        <KeyValueEditor
          entries={body.formData}
          onChange={setFormData}
          keyPlaceholder="Field name"
          valuePlaceholder="Value"
          addLabel="Add Field"
        />
      )}

      {body.mode === 'binary' && (
        body.filePath ? (
          <Card className="max-w-sm">
            <CardContent className="flex items-center gap-3 p-4">
              <FileUp className="size-5 shrink-0 text-muted-foreground" />
              <span className="flex-1 truncate text-sm">{body.fileName}</span>
              <Button variant="ghost" size="sm" onClick={handleClear}>
                Clear
              </Button>
            </CardContent>
          </Card>
        ) : (
          <Button variant="outline" onClick={handlePickFile}>
            <FileUp className="mr-2 size-4" />
            Choose file
          </Button>
        )
      )}
    </div>
  );
}
