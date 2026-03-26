import { useCallback, lazy, Suspense } from 'react';
import { FileUp } from 'lucide-react';
import { open } from '@tauri-apps/plugin-dialog';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { BodyState, KeyValueEntry } from '@/types/pane-types';
import { KeyValueEditor } from './KeyValueEditor';

// Lazy-load Monaco so it stays out of the initial JS bundle.
const MonacoWrapper = lazy(() =>
  import('@/components/editor/MonacoWrapper').then((m) => ({
    default: m.MonacoWrapper,
  })),
);

type BodyMode = BodyState['mode'];

interface BodyEditorProps {
  body: BodyState;
  onChange: (body: BodyState) => void;
}

const MODES: { label: string; value: BodyMode }[] = [
  { label: 'None', value: 'none' },
  { label: 'JSON', value: 'json' },
  { label: 'XML', value: 'xml' },
  { label: 'Text', value: 'text' },
  { label: 'Form Data', value: 'formdata' },
  { label: 'Binary', value: 'binary' },
];

export function BodyEditor({ body, onChange }: BodyEditorProps) {
  const setMode = useCallback(
    (mode: BodyMode) => onChange({ ...body, mode }),
    [body, onChange],
  );

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
      {/* Mode selector dropdown. */}
      <div className="flex items-center gap-2 shrink-0">
        <Select value={body.mode} onValueChange={(val) => setMode(val as BodyMode)}>
          <SelectTrigger className="w-[140px] h-7 text-xs">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {MODES.map((m) => (
              <SelectItem key={m.value} value={m.value} className="text-xs">
                {m.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

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
