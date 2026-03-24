import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { X } from 'lucide-react';
import { cn } from '@/lib/utils';
import type { Tab, HttpMethod } from '@/types/pane-types';

const METHOD_COLORS: Record<HttpMethod, string> = {
  GET: 'bg-emerald-500/15 text-emerald-700 dark:text-emerald-400 border-emerald-200 dark:border-emerald-800',
  POST: 'bg-amber-500/15 text-amber-700 dark:text-amber-400 border-amber-200 dark:border-amber-800',
  PUT: 'bg-blue-500/15 text-blue-700 dark:text-blue-400 border-blue-200 dark:border-blue-800',
  PATCH: 'bg-purple-500/15 text-purple-700 dark:text-purple-400 border-purple-200 dark:border-purple-800',
  DELETE: 'bg-red-500/15 text-red-700 dark:text-red-400 border-red-200 dark:border-red-800',
  OPTIONS: 'bg-gray-500/15 text-gray-700 dark:text-gray-400 border-gray-200 dark:border-gray-800',
  HEAD: 'bg-gray-500/15 text-gray-700 dark:text-gray-400 border-gray-200 dark:border-gray-800',
};

interface TabItemProps {
  tab: Tab;
  isActive: boolean;
  onSelect: () => void;
  onClose: () => void;
}

// Derives a display title from the tab state.
function getTabTitle(tab: Tab): string {
  if (tab.title && tab.title !== 'New request' && tab.request.url) {
    return tab.title;
  }
  if (tab.request.url) {
    try {
      return new URL(tab.request.url).hostname;
    } catch {
      return tab.request.url;
    }
  }
  return 'New request';
}

// Single tab element with method badge, title, dirty indicator, and close button.
export function TabItem({ tab, isActive, onSelect, onClose }: TabItemProps) {
  return (
    <button
      role="tab"
      aria-selected={isActive}
      tabIndex={isActive ? 0 : -1}
      onClick={onSelect}
      className={cn(
        'group flex items-center gap-1.5 px-3 py-1.5 text-sm border-b-2 transition-colors',
        isActive
          ? 'border-primary text-foreground bg-background'
          : 'border-transparent text-muted-foreground hover:text-foreground hover:bg-accent/50',
      )}
    >
      <Badge
        variant="outline"
        className={cn(
          'text-[10px] px-1 py-0 font-semibold leading-none',
          METHOD_COLORS[tab.request.method],
        )}
      >
        {tab.request.method}
      </Badge>
      <span className="max-w-32 truncate">{getTabTitle(tab)}</span>
      {tab.isDirty && (
        <span
          className="size-1.5 shrink-0 rounded-full bg-primary"
          aria-label="Unsaved changes"
        />
      )}
      <Button
        variant="ghost"
        size="icon"
        className="size-4 opacity-0 group-hover:opacity-100 aria-selected:opacity-100"
        onClick={(e) => {
          e.stopPropagation();
          onClose();
        }}
      >
        <X className="size-3" />
      </Button>
    </button>
  );
}
