import { useCallback, useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { requestEntriesOf, type SavedRequestEntry } from '@/lib/flow-node-edits';
import { getCollection } from '@/lib/tauri-api';

// A searchable list of this collection's requests. The tree is fetched each
// time the popover opens, so it never shows a stale list.
export function RequestPicker({
  collection,
  triggerLabel,
  onPick,
}: {
  collection: string;
  triggerLabel: string;
  onPick: (entry: SavedRequestEntry) => void;
}) {
  const [open, setOpen] = useState(false);
  const [entries, setEntries] = useState<SavedRequestEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState('');

  const load = useCallback(() => {
    setError(null);
    setEntries(null);
    getCollection(collection)
      .then((c) => setEntries(requestEntriesOf(c.root)))
      .catch((err) => setError(String(err)));
  }, [collection]);

  useEffect(() => {
    if (open) load();
  }, [open, load]);

  const query = filter.trim().toLowerCase();
  const shown = (entries ?? []).filter(
    (e) => !query || e.name.toLowerCase().includes(query) || e.path.toLowerCase().includes(query),
  );

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button type='button' variant='outline' size='sm' className='h-7 text-xs'>
          {triggerLabel}
        </Button>
      </PopoverTrigger>
      <PopoverContent align='start' className='nokey w-72 p-2'>
        <Input
          aria-label='Filter requests'
          placeholder='Filter requests'
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          className='mb-2 h-7 text-xs'
        />
        {error ? (
          <div
            role='alert'
            className='flex items-center justify-between gap-2 text-xs text-red-600'
          >
            <span>Could not load requests: {error}</span>
            <Button
              type='button'
              variant='outline'
              size='sm'
              className='h-6 text-xs'
              onClick={load}
            >
              Retry
            </Button>
          </div>
        ) : entries === null ? (
          <p className='text-xs text-muted-foreground'>Loading…</p>
        ) : shown.length === 0 ? (
          <p className='text-xs text-muted-foreground'>No matching requests.</p>
        ) : (
          <div className='max-h-60 space-y-0.5 overflow-y-auto'>
            {shown.map((entry) => (
              <Button
                key={entry.path}
                type='button'
                variant='ghost'
                size='sm'
                className='h-7 w-full justify-start gap-2 text-xs'
                onClick={() => {
                  onPick(entry);
                  setOpen(false);
                }}
              >
                <span className='font-mono text-[10px] text-muted-foreground'>{entry.method}</span>
                <span className='truncate'>{entry.name}</span>
              </Button>
            ))}
          </div>
        )}
      </PopoverContent>
    </Popover>
  );
}
