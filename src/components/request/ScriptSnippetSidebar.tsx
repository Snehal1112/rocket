import { ChevronDown, ChevronRight } from 'lucide-react';
import { useCallback, useRef, useState } from 'react';
import type {
  ScriptSnippetGroup,
  ScriptSnippetItem,
  ScriptSnippetSubGroup,
} from '@/components/editor/rok-types';
import { ROK_SNIPPETS } from '@/components/editor/rok-types';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible';
import { ScrollArea } from '@/components/ui/scroll-area';

interface ScriptSnippetSidebarProps {
  onInsert: (code: string) => void;
  snippets?: ScriptSnippetGroup[];
}

const MIN_WIDTH = 120;
const MAX_WIDTH_FRACTION = 0.5;
const DEFAULT_WIDTH = 220;

function SnippetItem({
  item,
  onInsert,
}: {
  item: ScriptSnippetItem;
  onInsert: (code: string) => void;
}) {
  return (
    <button
      type='button'
      onClick={() => onInsert(item.code)}
      className='w-full text-left px-3 py-1 text-xs text-muted-foreground hover:bg-accent hover:text-accent-foreground rounded-sm truncate'
      title={item.code}
    >
      {item.label}
    </button>
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
}: ScriptSnippetSidebarProps) {
  const [width, setWidth] = useState(DEFAULT_WIDTH);
  const dragging = useRef(false);
  const startX = useRef(0);
  const startWidth = useRef(DEFAULT_WIDTH);

  const onMouseDown = useCallback(
    (e: React.MouseEvent) => {
      dragging.current = true;
      startX.current = e.clientX;
      startWidth.current = width;

      const onMouseMove = (ev: MouseEvent) => {
        if (!dragging.current) return;
        const delta = startX.current - ev.clientX;
        const containerWidth = document.body.clientWidth;
        const maxWidth = containerWidth * MAX_WIDTH_FRACTION;
        const next = Math.min(maxWidth, Math.max(MIN_WIDTH, startWidth.current + delta));
        setWidth(next);
      };

      const onMouseUp = () => {
        dragging.current = false;
        window.removeEventListener('mousemove', onMouseMove);
        window.removeEventListener('mouseup', onMouseUp);
      };

      window.addEventListener('mousemove', onMouseMove);
      window.addEventListener('mouseup', onMouseUp);
    },
    [width],
  );

  return (
    <div className='flex h-full shrink-0' style={{ width }}>
      {/* Drag handle. */}
      {/* biome-ignore lint/a11y/useSemanticElements: drag splitter cannot be an <hr> */}
      <div
        onMouseDown={onMouseDown}
        className='w-1 shrink-0 cursor-col-resize hover:bg-primary/40 transition-colors bg-border'
        role='separator'
        tabIndex={0}
        aria-orientation='vertical'
        aria-label='Resize sidebar'
        aria-valuenow={width}
        aria-valuemin={MIN_WIDTH}
        aria-valuemax={800}
      />
      <div className='flex flex-col flex-1 min-w-0 border-l'>
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
