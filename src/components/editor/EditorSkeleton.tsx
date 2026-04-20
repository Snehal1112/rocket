import { cn } from '@/lib/utils';

const LINE_WIDTHS = ['w-3/4', 'w-1/2', 'w-5/6', 'w-2/5', 'w-3/5', 'w-1/3', 'w-4/5'];

export function EditorSkeleton() {
  return (
    <div
      className='flex h-full w-full overflow-hidden bg-background font-mono text-xs'
      aria-hidden='true'
    >
      {/* Line numbers column. */}
      <div className='flex w-10 shrink-0 flex-col gap-3 border-r border-border px-2 pt-3'>
        {[0, 1, 2, 3].map((n) => (
          <div key={n} className='h-2 w-6 motion-safe:animate-pulse rounded-sm bg-muted' />
        ))}
      </div>

      {/* Code content area with shimmer lines. */}
      <div className='flex flex-1 motion-safe:animate-pulse flex-col gap-3 p-3'>
        {LINE_WIDTHS.map((width) => (
          <div key={width} className={cn('h-2 rounded-sm bg-muted/50', width)} />
        ))}
      </div>
    </div>
  );
}
