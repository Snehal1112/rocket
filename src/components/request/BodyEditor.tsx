import { useCallback } from 'react';
import { Trash2, Plus, FileUp } from 'lucide-react';
import { open } from '@tauri-apps/plugin-dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Checkbox } from '@/components/ui/checkbox';
import { Card, CardContent } from '@/components/ui/card';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
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
    <div className="space-y-3">
      {/* Mode selector dropdown. */}
      <Select value={body.mode} onValueChange={(val) => setMode(val as BodyMode)}>
        <SelectTrigger className="h-8 w-[140px] text-xs">
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

      {/* Content area. */}
      {body.mode === 'none' && (
        <p className="text-xs text-muted-foreground">
          This request has no body.
        </p>
      )}

      {(body.mode === 'json' || body.mode === 'xml' || body.mode === 'text') && (
        <MonacoWrapper
          value={body.content}
          onChange={(val) => setContent(val)}
          bodyMode={body.mode}
          height="250px"
        />
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

// Sub-component for form data entries using shadcn Table.
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
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead className="h-7 text-xs">Key</TableHead>
            <TableHead className="h-7 text-xs">Value</TableHead>
            <TableHead className="h-7 w-10 text-center text-xs">On</TableHead>
            <TableHead className="h-7 w-8" />
          </TableRow>
        </TableHeader>
        <TableBody>
          {formData.map((entry) => (
            <TableRow key={entry.id}>
              <TableCell className="p-1">
                <Input
                  className="h-7 text-xs"
                  placeholder="field name"
                  value={entry.key}
                  onChange={(e) => updateEntry(entry.id, { key: e.target.value })}
                />
              </TableCell>
              <TableCell className="p-1">
                <Input
                  className="h-7 text-xs"
                  placeholder="field value"
                  value={entry.value}
                  onChange={(e) => updateEntry(entry.id, { value: e.target.value })}
                />
              </TableCell>
              <TableCell className="p-1 text-center">
                <Checkbox
                  checked={entry.enabled}
                  onCheckedChange={(checked) =>
                    updateEntry(entry.id, { enabled: checked === true })
                  }
                  aria-label={`Enable field ${entry.key || 'unnamed'}`}
                  className="h-3.5 w-3.5"
                />
              </TableCell>
              <TableCell className="p-1">
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7"
                  onClick={() => removeEntry(entry.id)}
                  aria-label={`Remove field ${entry.key || 'unnamed'}`}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </Button>
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>

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
