import type { Tab, HttpMethod } from '@/types/pane-types';
import { usePaneStore } from '@/stores/pane-store';
import { X } from 'lucide-react';
import { cn } from '@/lib/utils';

interface TabItemProps {
  tab: Tab;
  groupId: string;
  isActive: boolean;
}

const METHOD_COLORS: Record<HttpMethod, string> = {
  GET: 'bg-emerald-500/15 text-emerald-600 dark:text-emerald-400',
  POST: 'bg-amber-500/15 text-amber-600 dark:text-amber-400',
  PUT: 'bg-blue-500/15 text-blue-600 dark:text-blue-400',
  PATCH: 'bg-purple-500/15 text-purple-600 dark:text-purple-400',
  DELETE: 'bg-red-500/15 text-red-600 dark:text-red-400',
  OPTIONS: 'bg-gray-500/15 text-gray-600 dark:text-gray-400',
  HEAD: 'bg-gray-500/15 text-gray-600 dark:text-gray-400',
};

// Derives a display title from the tab state.
function getTabTitle(tab: Tab): string {
  if (tab.title) return tab.title;
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
export function TabItem({ tab, groupId, isActive }: TabItemProps) {
  const setActiveTab = usePaneStore((s) => s.setActiveTab);
  const closeTab = usePaneStore((s) => s.closeTab);

  return (
    <div
      role="tab"
      aria-selected={isActive}
      tabIndex={isActive ? 0 : -1}
      onClick={() => setActiveTab(tab.id, groupId)}
      className={cn(
        'group flex cursor-pointer items-center gap-1.5 border-r border-border px-3 py-1.5 text-sm',
        'transition-colors hover:bg-accent/50',
        isActive
          ? 'bg-background text-foreground shadow-[inset_0_-2px_0_0] shadow-primary'
          : 'text-muted-foreground',
      )}
    >
      {/* Method badge. */}
      <span
        className={cn(
          'shrink-0 rounded px-1 py-0.5 text-[10px] font-semibold leading-none',
          METHOD_COLORS[tab.request.method],
        )}
      >
        {tab.request.method}
      </span>

      {/* Title. */}
      <span className="max-w-32 truncate">{getTabTitle(tab)}</span>

      {/* Dirty indicator. */}
      {tab.isDirty && (
        <span
          className="size-1.5 shrink-0 rounded-full bg-primary"
          aria-label="Unsaved changes"
        />
      )}

      {/* Close button. */}
      <button
        type="button"
        aria-label={`Close ${getTabTitle(tab)}`}
        onClick={(e) => {
          e.stopPropagation();
          closeTab(tab.id, groupId);
        }}
        className={cn(
          'ml-0.5 shrink-0 rounded p-0.5 transition-colors',
          'opacity-0 hover:bg-muted group-hover:opacity-100',
          isActive && 'opacity-100',
        )}
      >
        <X className="size-3" />
      </button>
    </div>
  );
}
