import { ChevronDown, ChevronUp, X } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { searchFlowNodes } from '@/lib/flow-search';
import type { FlowNode } from '@/lib/tauri-api';

interface FlowSearchBarProps {
  nodes: FlowNode[];
  // Changes whenever the bar is asked to take focus again, for example by a second Ctrl+F.
  focusToken: number;
  // Called with the node to select and show.
  onShowMatch: (nodeId: string) => void;
  onClose: () => void;
}

// The search field of a flow canvas. The owner selects and zooms to a match.
// The classes keep a click or key press here from panning, dragging or deleting on the canvas.
export function FlowSearchBar({ nodes, focusToken, onShowMatch, onClose }: FlowSearchBarProps) {
  const [query, setQuery] = useState('');
  const [index, setIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const matches = useMemo(() => searchFlowNodes(nodes, query), [nodes, query]);
  // The graph can change while the bar is open, so keep the index inside the matches.
  const current = Math.min(index, Math.max(matches.length - 1, 0));

  // biome-ignore lint/correctness/useExhaustiveDependencies: the token is the trigger.
  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, [focusToken]);

  const show = (next: number) => {
    if (matches.length === 0) return;
    const wrapped = (next + matches.length) % matches.length;
    setIndex(wrapped);
    onShowMatch(matches[wrapped]);
  };

  const handleChange = (value: string) => {
    setQuery(value);
    setIndex(0);
    const first = searchFlowNodes(nodes, value)[0];
    if (first) onShowMatch(first);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      show(current + (e.shiftKey ? -1 : 1));
    }
  };

  const position = matches.length === 0 ? 0 : current + 1;
  const counter = query.trim() === '' ? '' : `${position} of ${matches.length}`;

  return (
    <div className='nokey nodrag nopan nowheel flex items-center gap-1 rounded-md border bg-card p-1 shadow-sm'>
      <Input
        ref={inputRef}
        aria-label='Search nodes'
        placeholder='Search nodes'
        className='h-7 w-44 text-xs'
        value={query}
        onChange={(e) => handleChange(e.target.value)}
        onKeyDown={handleKeyDown}
      />
      <span
        role='status'
        aria-live='polite'
        className='min-w-10 text-center text-[11px] text-muted-foreground'
      >
        {counter}
      </span>
      <Button
        type='button'
        size='icon'
        variant='ghost'
        className='h-7 w-7'
        aria-label='Previous match'
        disabled={matches.length === 0}
        onClick={() => show(current - 1)}
      >
        <ChevronUp className='h-3.5 w-3.5' aria-hidden='true' />
      </Button>
      <Button
        type='button'
        size='icon'
        variant='ghost'
        className='h-7 w-7'
        aria-label='Next match'
        disabled={matches.length === 0}
        onClick={() => show(current + 1)}
      >
        <ChevronDown className='h-3.5 w-3.5' aria-hidden='true' />
      </Button>
      <Button
        type='button'
        size='icon'
        variant='ghost'
        className='h-7 w-7'
        aria-label='Close search'
        onClick={onClose}
      >
        <X className='h-3.5 w-3.5' aria-hidden='true' />
      </Button>
    </div>
  );
}
