import { Plus, X } from 'lucide-react';
import { lazy, Suspense } from 'react';
import { EditorSkeleton } from '@/components/editor/EditorSkeleton';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import type { VariableScopeEntry } from '@/lib/url-variables';
import type { GrpcMessageState } from '@/types/pane-types';

const MonacoWrapper = lazy(() =>
  import('@/components/editor/MonacoWrapper').then((m) => ({ default: m.MonacoWrapper })),
);

interface GrpcMessageEditorProps {
  messages: GrpcMessageState[];
  active: number;
  onSelect: (index: number) => void;
  onChange: (index: number, patch: Partial<Pick<GrpcMessageState, 'title' | 'content'>>) => void;
  onAdd: () => void;
  onRemove: (index: number) => void;
  variableContext?: Map<string, VariableScopeEntry>;
}

/** Edits the saved messages of a request, one at a time, as protobuf JSON. */
export function GrpcMessageEditor({
  messages,
  active,
  onSelect,
  onChange,
  onAdd,
  onRemove,
  variableContext,
}: GrpcMessageEditorProps) {
  const current = messages[active];
  return (
    <div className='flex h-full min-h-0 flex-col gap-2'>
      <div className='flex flex-wrap items-center gap-1'>
        {messages.map((m, i) => (
          <Button
            key={m.id}
            type='button'
            size='sm'
            variant={i === active ? 'secondary' : 'ghost'}
            className='h-7 px-2 text-xs'
            aria-pressed={i === active}
            onClick={() => onSelect(i)}
          >
            {m.title || `Message ${i + 1}`}
          </Button>
        ))}
        <Button
          type='button'
          size='sm'
          variant='ghost'
          className='h-7 px-2'
          aria-label='Add message'
          onClick={onAdd}
        >
          <Plus className='h-3.5 w-3.5' aria-hidden='true' />
        </Button>
        {messages.length > 1 && (
          <Button
            type='button'
            size='sm'
            variant='ghost'
            className='h-7 px-2'
            aria-label='Remove message'
            onClick={() => onRemove(active)}
          >
            <X className='h-3.5 w-3.5' aria-hidden='true' />
          </Button>
        )}
        <Input
          aria-label='Message title'
          placeholder='Title'
          value={current?.title ?? ''}
          onChange={(e) => onChange(active, { title: e.target.value })}
          className='ml-auto h-7 w-40 text-xs'
        />
      </div>
      <div className='min-h-[140px] flex-1'>
        {current && (
          <Suspense fallback={<EditorSkeleton />}>
            <MonacoWrapper
              value={current.content}
              onChange={(value) => onChange(active, { content: value })}
              language='json'
              height='100%'
              variableContext={variableContext}
            />
          </Suspense>
        )}
      </div>
    </div>
  );
}
