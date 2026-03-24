import { useCallback } from 'react';
import { Check, X, Plus, FileUp } from 'lucide-react';
import { open } from '@tauri-apps/plugin-dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Card, CardContent } from '@/components/ui/card';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { MonacoWrapper } from '@/components/editor/MonacoWrapper';
import type { BodyState, KeyValueEntry } from '@/types/pane-types';

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
          <MonacoWrapper
            value={body.content}
            onChange={(val) => setContent(val)}
            bodyMode={body.mode}
            height="100%"
          />
        </div>
      )}

      {body.mode === 'formdata' && (
        <FormDataEditor formData={body.formData} onChange={setFormData} />
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

// Sub-component for form data entries using flex rows matching legacy pattern.
function FormDataEditor({
  formData,
  onChange,
}: {
  formData: KeyValueEntry[];
  onChange: (data: KeyValueEntry[]) => void;
}) {
  const updateEntry = useCallback(
    (id: string, patch: Partial<KeyValueEntry>) => {
      onChange(formData.map((e) => (e.id === id ? { ...e, ...patch } : e)));
    },
    [formData, onChange],
  );

  const removeEntry = useCallback(
    (id: string) => {
      onChange(formData.filter((e) => e.id !== id));
    },
    [formData, onChange],
  );

  const addEntry = useCallback(() => {
    onChange([
      ...formData,
      { id: crypto.randomUUID(), key: '', value: '', enabled: true },
    ]);
  }, [formData, onChange]);

  return (
    <div className="space-y-2">
      {formData.map((entry) => (
        <div key={entry.id} className="flex gap-2 items-center">
          <Button
            variant="ghost"
            size="icon"
            onClick={() => updateEntry(entry.id, { enabled: !entry.enabled })}
            className={`w-4 h-4 rounded border p-0 ${
              entry.enabled
                ? 'bg-primary border-primary text-primary-foreground hover:bg-primary/90'
                : 'border-gray-300 hover:bg-muted'
            }`}
            aria-label={`${entry.enabled ? 'Disable' : 'Enable'} field ${entry.key || 'unnamed'}`}
          >
            {entry.enabled && <Check className="h-3 w-3" />}
          </Button>
          <Input
            placeholder="Key"
            value={entry.key}
            onChange={(e) => updateEntry(entry.id, { key: e.target.value })}
            className="flex-1 text-xs h-8"
          />
          <Input
            placeholder="Value"
            value={entry.value}
            onChange={(e) => updateEntry(entry.id, { value: e.target.value })}
            className="flex-1 text-xs h-8"
          />
          <Button
            variant="ghost"
            size="icon"
            onClick={() => removeEntry(entry.id)}
            className="h-7 w-7"
            aria-label={`Remove field ${entry.key || 'unnamed'}`}
          >
            <X className="h-4 w-4" />
          </Button>
        </div>
      ))}
      <Button variant="ghost" size="sm" onClick={addEntry} className="text-xs">
        <Plus className="h-3 w-3 mr-1" />
        Add Field
      </Button>
    </div>
  );
}
