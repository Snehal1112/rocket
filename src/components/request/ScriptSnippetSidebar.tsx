import { ChevronDown, ChevronRight } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import type {
  ScriptSnippetGroup,
  ScriptSnippetItem,
  ScriptSnippetSubGroup,
} from '@/components/editor/rok-types';
import { ROK_SNIPPETS } from '@/components/editor/rok-types';
import { Button } from '@/components/ui/button';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible';
import { ScrollArea } from '@/components/ui/scroll-area';
import { cn } from '@/lib/utils';

interface ScriptSnippetSidebarProps {
  onInsert: (code: string) => void;
  snippets?: ScriptSnippetGroup[];
  maxWidth?: number;
}

const MIN_WIDTH = 160;
const DEFAULT_WIDTH = 220;
const RESIZE_STEP = 16;

function SnippetItem({
  item,
  onInsert,
}: {
  item: ScriptSnippetItem;
  onInsert: (code: string) => void;
}) {
  return (
    <Button
      type='button'
      variant='ghost'
      size='sm'
      onClick={() => onInsert(item.code)}
      className='h-auto w-full justify-start rounded-sm px-3 py-1 text-left text-xs font-normal text-muted-foreground hover:text-accent-foreground'
      title={item.code}
    >
      <span className='truncate'>{item.label}</span>
    </Button>
  );
}

function SubGroupSection({
  sub,
  onInsert,
}: {
  sub: ScriptSnippetSubGroup;
  onInsert: (code: string) => void;
}) {
  const [open, setOpen] = useState(true);
  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <CollapsibleTrigger className='flex items-center gap-1 w-full px-3 py-1 text-xs font-medium text-foreground hover:bg-accent rounded-sm'>
        {open ? (
          <ChevronDown className='h-3 w-3 shrink-0' />
        ) : (
          <ChevronRight className='h-3 w-3 shrink-0' />
        )}
        {sub.label}
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className='pl-2'>
          {sub.items.map((item) => (
            <SnippetItem key={item.label} item={item} onInsert={onInsert} />
          ))}
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}

function GroupSection({
  group,
  onInsert,
}: {
  group: ScriptSnippetGroup;
  onInsert: (code: string) => void;
}) {
  const [open, setOpen] = useState(true);
  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <CollapsibleTrigger className='flex items-center gap-1 w-full px-3 py-1.5 text-xs font-semibold text-foreground hover:bg-accent rounded-sm'>
        {open ? (
          <ChevronDown className='h-3 w-3 shrink-0' />
        ) : (
          <ChevronRight className='h-3 w-3 shrink-0' />
        )}
        {group.label}
      </CollapsibleTrigger>
      <CollapsibleContent>
        {group.items?.map((item) => (
          <SnippetItem key={item.label} item={item} onInsert={onInsert} />
        ))}
        {group.subGroups?.map((sub) => (
          <SubGroupSection key={sub.id} sub={sub} onInsert={onInsert} />
        ))}
      </CollapsibleContent>
    </Collapsible>
  );
}

export function ScriptSnippetSidebar({
  onInsert,
  snippets = ROK_SNIPPETS,
  maxWidth = 400,
}: ScriptSnippetSidebarProps) {
  const effectiveMaxWidth = Math.max(MIN_WIDTH, maxWidth);
  const clampWidth = useCallback(
    (nextWidth: number) => Math.min(effectiveMaxWidth, Math.max(MIN_WIDTH, nextWidth)),
    [effectiveMaxWidth],
  );
  const [width, setWidth] = useState(() => clampWidth(DEFAULT_WIDTH));
  const [isDragging, setIsDragging] = useState(false);
  const dragging = useRef(false);
  const startX = useRef(0);
  const startWidth = useRef(width);

  useEffect(() => {
    setWidth((currentWidth) => clampWidth(currentWidth));
  }, [clampWidth]);

  const onPointerDown = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      event.preventDefault();
      dragging.current = true;
      setIsDragging(true);
      startX.current = event.clientX;
      startWidth.current = width;
      event.currentTarget.setPointerCapture(event.pointerId);
    },
    [width],
  );

  const onPointerMove = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (!dragging.current) return;
      setWidth(clampWidth(startWidth.current + startX.current - event.clientX));
    },
    [clampWidth],
  );

  const stopDragging = useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    dragging.current = false;
    setIsDragging(false);
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  }, []);

  const onKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      switch (event.key) {
        case 'ArrowLeft':
          event.preventDefault();
          setWidth((currentWidth) => clampWidth(currentWidth - RESIZE_STEP));
          break;
        case 'ArrowRight':
          event.preventDefault();
          setWidth((currentWidth) => clampWidth(currentWidth + RESIZE_STEP));
          break;
        case 'Home':
          event.preventDefault();
          setWidth(MIN_WIDTH);
          break;
        case 'End':
          event.preventDefault();
          setWidth(effectiveMaxWidth);
          break;
      }
    },
    [clampWidth, effectiveMaxWidth],
  );

  return (
    <div id='script-snippet-sidebar' className='flex shrink-0 self-stretch' style={{ width }}>
      {/* biome-ignore lint/a11y/useSemanticElements: drag splitter cannot be an <hr> */}
      <div
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={stopDragging}
        onPointerCancel={stopDragging}
        onKeyDown={onKeyDown}
        className={cn(
          'flex w-3 shrink-0 cursor-col-resize select-none items-center justify-center border-r transition-colors',
          'focus-visible:outline-2 focus-visible:outline-ring focus-visible:-outline-offset-2',
          isDragging
            ? 'border-primary/50 bg-primary/15'
            : 'border-border bg-muted/30 hover:border-primary/40 hover:bg-accent/50',
        )}
        role='separator'
        tabIndex={0}
        aria-orientation='vertical'
        aria-label='Resize snippets sidebar'
        aria-valuenow={Math.round(width)}
        aria-valuemin={MIN_WIDTH}
        aria-valuemax={Math.round(effectiveMaxWidth)}
      >
        <div
          className={cn(
            'h-16 rounded-full transition-all',
            isDragging ? 'w-1.5 bg-primary' : 'w-1 bg-muted-foreground/40',
          )}
        />
      </div>
      <div className='flex min-w-0 flex-1 flex-col border-l'>
        <div className='px-3 py-2 text-xs font-semibold text-muted-foreground uppercase tracking-wide border-b shrink-0'>
          Snippets
        </div>
        <ScrollArea className='flex-1'>
          <div className='py-1'>
            {snippets.map((group) => (
              <GroupSection key={group.id} group={group} onInsert={onInsert} />
            ))}
          </div>
        </ScrollArea>
      </div>
    </div>
  );
}
