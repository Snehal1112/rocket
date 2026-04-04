import { useEffect, useState } from 'react';
import { cn } from '@/lib/utils';

// Tracks dark mode by observing the html element class changes.
function useIsDark(): boolean {
  const [isDark, setIsDark] = useState(() => document.documentElement.classList.contains('dark'));

  useEffect(() => {
    const observer = new MutationObserver(() => {
      setIsDark(document.documentElement.classList.contains('dark'));
    });
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['class'],
    });
    return () => observer.disconnect();
  }, []);

  return isDark;
}

// Widths for the fake code lines — varying to feel like real code.
const LINE_WIDTHS = ['w-3/4', 'w-1/2', 'w-5/6', 'w-2/5', 'w-3/5', 'w-1/3', 'w-4/5'];

// Placeholder shown while Monaco loads — matches Monaco's background exactly.
export function EditorSkeleton() {
  const isDark = useIsDark();

  return (
    <div
      className={cn(
        'flex h-full w-full overflow-hidden font-mono text-xs',
        isDark ? 'bg-[#1f1f1f]' : 'bg-white',
      )}
      aria-hidden='true'
    >
      {/* Line numbers column. */}
      <div
        className={cn(
          'flex w-10 shrink-0 flex-col gap-3 px-2 pt-3',
          isDark ? 'border-r border-[#333]' : 'border-r border-[#e4e4e4]',
        )}
      >
        {[0, 1, 2, 3].map((n) => (
          <div
            key={n}
            className='h-2 w-6 animate-pulse rounded-sm'
            style={{ background: isDark ? '#444' : '#ddd' }}
          />
        ))}
      </div>

      {/* Code content area with shimmer lines. */}
      <div className='flex flex-1 animate-pulse flex-col gap-3 p-3'>
        {LINE_WIDTHS.map((width) => (
          <div
            key={width}
            className={cn('h-2 rounded-sm', width)}
            style={{ background: isDark ? '#2d2d2d' : '#ececec' }}
          />
        ))}
      </div>
    </div>
  );
}
