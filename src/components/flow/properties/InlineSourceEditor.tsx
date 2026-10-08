import { Plus, X } from 'lucide-react';
import { SingleLineEditor } from '@/components/editor';
import { MonacoWrapper } from '@/components/editor/MonacoWrapper';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { FlowEdge, InlineHeader, InlineRequestData } from '@/lib/tauri-api';
import { useFlowVariableContext } from './flowVariableContext';
import { usePanelRefocus } from './panelFocus';

const METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS'] as const;

const HEADER_INDEX = /^headers\[(\d+)\]/;

// Turns index wires into the one-based positions a person reads.
function missingPositions(wires: FlowEdge[]): number[] {
  return wires
    .map((w) => HEADER_INDEX.exec(w.targetField))
    .filter((m): m is RegExpExecArray => m !== null)
    .map((m) => Number(m[1]) + 1);
}

export function InlineSourceEditor({
  request,
  onChange,
  outOfRangeWires,
}: {
  request: InlineRequestData;
  onChange: (request: InlineRequestData) => void;
  outOfRangeWires: FlowEdge[];
}) {
  const refocusPanel = usePanelRefocus();
  const variableContext = useFlowVariableContext();
  const setHeader = (index: number, patch: Partial<InlineHeader>) =>
    onChange({
      ...request,
      headers: request.headers.map((h, i) => (i === index ? { ...h, ...patch } : h)),
    });
  // Rows are keyed by index, so only the last row unmounts. Its remove button
  // held focus, so focus moves to the panel first.
  const removeHeader = (index: number) => {
    if (index === request.headers.length - 1) refocusPanel();
    onChange({ ...request, headers: request.headers.filter((_, j) => j !== index) });
  };
  const contentType = request.headers.find((h) => h.name.toLowerCase() === 'content-type')?.value;
  const positions = missingPositions(outOfRangeWires);
  // A method outside the list still gets an item, so the select never shows
  // blank. Radix rejects an empty item value, so an empty method gets none.
  const listed = !request.method || (METHODS as readonly string[]).includes(request.method);
  const methods: readonly string[] = listed ? METHODS : [...METHODS, request.method];

  return (
    <div className='space-y-3'>
      <div className='flex items-center gap-2'>
        <Select value={request.method} onValueChange={(method) => onChange({ ...request, method })}>
          <SelectTrigger aria-label='Method' className='h-8 w-28 text-xs'>
            <SelectValue />
          </SelectTrigger>
          <SelectContent className='nokey'>
            {methods.map((m) => (
              <SelectItem key={m} value={m}>
                {m}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <SingleLineEditor
          aria-label='URL'
          className='flex-1'
          value={request.url}
          onChange={(url) => onChange({ ...request, url })}
          placeholder='https://api.example.com/{{path}}'
          variableContext={variableContext}
          readOnlyVariables
        />
      </div>

      <div className='space-y-1'>
        <div className='flex items-center justify-between'>
          <span className='text-xs font-medium'>Headers</span>
          <Button
            type='button'
            variant='ghost'
            size='sm'
            className='h-6 gap-1 text-xs'
            onClick={() =>
              onChange({ ...request, headers: [...request.headers, { name: '', value: '' }] })
            }
          >
            <Plus className='h-3 w-3' aria-hidden='true' />
            Add header
          </Button>
        </div>
        {request.headers.map((header, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: inline headers have no id, and each row is fully controlled by its index.
          <div key={i} className='flex items-center gap-1'>
            <SingleLineEditor
              aria-label={`Header ${i + 1} name`}
              className='flex-1'
              value={header.name}
              onChange={(name) => setHeader(i, { name })}
              placeholder='Name'
              variableContext={variableContext}
              readOnlyVariables
            />
            <SingleLineEditor
              aria-label={`Header ${i + 1} value`}
              className='flex-1'
              value={header.value}
              onChange={(value) => setHeader(i, { value })}
              placeholder='Value'
              variableContext={variableContext}
              readOnlyVariables
            />
            <Button
              type='button'
              variant='ghost'
              size='icon'
              className='h-6 w-6 shrink-0'
              aria-label={`Remove header ${i + 1}`}
              onClick={() => removeHeader(i)}
            >
              <X className='h-3 w-3' aria-hidden='true' />
            </Button>
          </div>
        ))}
        {positions.length > 0 && (
          <p role='alert' className='text-xs text-amber-600 dark:text-amber-400'>
            {positions.length === 1
              ? `A wire targets header position ${positions[0]}, which no longer exists.`
              : `Wires target header positions ${positions.join(', ')}, which no longer exist.`}{' '}
            This node will fail when the flow runs.
          </p>
        )}
      </div>

      <div className='space-y-1'>
        <span className='text-xs font-medium'>Body</span>
        <div className='h-40 overflow-hidden rounded border'>
          <MonacoWrapper
            value={request.body ?? ''}
            onChange={(body) => onChange({ ...request, body: body === '' ? null : body })}
            contentType={contentType}
            variableContext={variableContext}
            height='100%'
          />
        </div>
      </div>
    </div>
  );
}
