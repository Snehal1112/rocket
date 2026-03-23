import { useCallback } from 'react';
import { X, Plus, FileUp } from 'lucide-react';
import { open } from '@tauri-apps/plugin-dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { cn } from '@/lib/utils';
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
    <div className="space-y-3">
      {/* Mode selector tabs. */}
      <div className="flex gap-1 border-b border-border pb-1">
        {MODES.map((m) => (
          <button
            key={m.value}
            type="button"
            className={cn(
              'rounded-md px-2.5 py-1 text-xs font-medium transition-colors',
              body.mode === m.value
                ? 'bg-muted text-foreground'
                : 'text-muted-foreground hover:text-foreground',
            )}
            onClick={() => setMode(m.value)}
          >
            {m.label}
          </button>
        ))}
      </div>

      {/* Content area. */}
      {body.mode === 'none' && (
        <p className="text-xs text-muted-foreground">
          This request has no body.
        </p>
      )}

      {(body.mode === 'json' || body.mode === 'xml' || body.mode === 'text') && (
        <textarea
          className="h-48 w-full resize-y rounded-md border border-input bg-transparent px-3 py-2 font-mono text-xs shadow-xs focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px] outline-none"
          placeholder={
            body.mode === 'json'
              ? '{ "key": "value" }'
              : body.mode === 'xml'
                ? '<root></root>'
                : 'Plain text body...'
          }
          value={body.content}
          onChange={(e) => setContent(e.target.value)}
          aria-label={`${body.mode.toUpperCase()} body content`}
        />
      )}

      {body.mode === 'formdata' && (
        <FormDataEditor formData={body.formData} onChange={setFormData} />
      )}

      {body.mode === 'binary' && (
        <div className="flex items-center gap-2 py-1">
          {body.filePath ? (
            <>
              <FileUp className="size-4 text-muted-foreground" />
              <span className="text-sm">{body.fileName}</span>
              <Button variant="ghost" size="sm" onClick={handleClear}>
                Clear
              </Button>
            </>
          ) : (
            <Button variant="outline" onClick={handlePickFile}>
              <FileUp className="size-4 mr-2" />
              Choose file
            </Button>
          )}
        </div>
      )}
    </div>
  );
}

// Sub-component for form data entries.
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
    <div className="space-y-1">
      <div className="grid grid-cols-[1fr_1fr_2.5rem_2rem] gap-1 px-1 text-xs font-medium text-muted-foreground">
        <span>Key</span>
        <span>Value</span>
        <span className="text-center">On</span>
        <span />
      </div>

      {formData.map((entry) => (
        <div
          key={entry.id}
          className="grid grid-cols-[1fr_1fr_2.5rem_2rem] items-center gap-1"
        >
          <Input
            className="h-7 text-xs"
            placeholder="field name"
            value={entry.key}
            onChange={(e) => updateEntry(entry.id, { key: e.target.value })}
          />
          <Input
            className="h-7 text-xs"
            placeholder="field value"
            value={entry.value}
            onChange={(e) => updateEntry(entry.id, { value: e.target.value })}
          />
          <div className="flex justify-center">
            <input
              type="checkbox"
              className="h-3.5 w-3.5 accent-primary"
              checked={entry.enabled}
              onChange={(e) =>
                updateEntry(entry.id, { enabled: e.target.checked })
              }
              aria-label={`Enable field ${entry.key || 'unnamed'}`}
            />
          </div>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            onClick={() => removeEntry(entry.id)}
            aria-label={`Remove field ${entry.key || 'unnamed'}`}
          >
            <X className="h-3.5 w-3.5" />
          </Button>
        </div>
      ))}

      <Button
        variant="ghost"
        size="sm"
        className="mt-1 h-7 text-xs text-muted-foreground"
        onClick={addEntry}
      >
        <Plus className="mr-1 h-3.5 w-3.5" />
        Add field
      </Button>
    </div>
  );
}
