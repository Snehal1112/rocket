import { ChevronDown } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { cn } from '@/lib/utils';

export interface RocketTabDef {
  value: string;
  label: React.ReactNode;
  isActive: boolean;
  onClick: () => void;
}

interface RocketTabBarProps {
  tabs: RocketTabDef[];
  rightContent?: React.ReactNode;
}

export function RocketTabBar({ tabs, rightContent }: RocketTabBarProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const tabRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const moreRef = useRef<HTMLButtonElement>(null);
  const rightRef = useRef<HTMLDivElement>(null);
  const [visibleCount, setVisibleCount] = useState(tabs.length);

  useEffect(() => {
    tabRefs.current = tabRefs.current.slice(0, tabs.length);
  }, [tabs.length]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const measure = () => {
      const containerWidth = container.offsetWidth;
      const px = 12; // px-3 = 12px each side, used for both sides
      // Reserve space for the right content and the "more" button (estimate).
      const rightWidth = rightRef.current ? rightRef.current.offsetWidth + 8 : 0;
      // More button width: estimate 40px.
      const moreWidth = 40;
      const available = containerWidth - px * 2 - rightWidth;

      let count = 0;
      let used = 0;
      for (let i = 0; i < tabs.length; i++) {
        const el = tabRefs.current[i];
        if (!el) {
          count = i;
          break;
        }
        // mr-4 = 16px gap after each tab.
        const w = el.offsetWidth + 16;
        // If adding this tab would leave no room for "more" button and there are remaining tabs, stop.
        const remaining = tabs.length - (i + 1);
        if (used + w + (remaining > 0 ? moreWidth : 0) > available) {
          count = i;
          break;
        }
        used += w;
        count = i + 1;
      }
      setVisibleCount(Math.max(1, count));
    };

    const observer = new ResizeObserver(measure);
    observer.observe(container);
    measure();
    return () => observer.disconnect();
  }, [tabs]);

  const overflowTabs = tabs.slice(visibleCount);
  const overflowHasActive = overflowTabs.some((t) => t.isActive);

  return (
    <div
      ref={containerRef}
      className='flex items-center border-b border-border px-3 shrink-0 min-w-0'
      role='tablist'
    >
      {tabs.map((tab, i) => (
        <button
          key={tab.value}
          ref={(el) => {
            tabRefs.current[i] = el;
          }}
          type='button'
          role='tab'
          id={`tab-${tab.value}`}
          aria-selected={tab.isActive}
          aria-controls={`panel-${tab.value}`}
          onClick={tab.onClick}
          className={cn(
            'py-2 mr-4 text-sm border-b-2 -mb-px transition-colors whitespace-nowrap shrink-0',
            i >= visibleCount ? 'invisible absolute pointer-events-none' : '',
            tab.isActive
              ? 'border-primary text-foreground font-semibold'
              : 'border-transparent text-muted-foreground hover:text-foreground',
          )}
        >
          {tab.label}
        </button>
      ))}

      {overflowTabs.length > 0 && (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              ref={moreRef}
              variant='ghost'
              size='sm'
              className={cn(
                'h-7 px-1.5 py-0 text-sm shrink-0 -mb-px border-b-2',
                overflowHasActive
                  ? 'border-primary text-foreground font-semibold'
                  : 'border-transparent text-muted-foreground hover:text-foreground',
              )}
            >
              <ChevronDown className='h-3.5 w-3.5' />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align='start' className='min-w-36'>
            {overflowTabs.map((tab) => (
              <DropdownMenuItem
                key={tab.value}
                onClick={tab.onClick}
                className={cn(
                  'text-sm cursor-pointer',
                  tab.isActive && 'font-semibold text-foreground',
                )}
              >
                {tab.label}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      )}

      {rightContent && (
        <div ref={rightRef} className='ml-auto flex items-center gap-2 shrink-0'>
          {rightContent}
        </div>
      )}
    </div>
  );
}
