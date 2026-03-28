import { X, Folder, GitBranch } from 'lucide-react';
import type { Tab, HttpMethod } from '@/types/pane-types';
import { isRequestTab, isGitTab } from '@/types/pane-types';

// Method text colors matching legacy constants.
const METHOD_TEXT_COLORS: Record<HttpMethod, string> = {
  GET: 'text-green-600',
  POST: 'text-blue-600',
  PUT: 'text-orange-600',
  PATCH: 'text-yellow-600',
  DELETE: 'text-red-600',
  OPTIONS: 'text-gray-500',
  HEAD: 'text-gray-500',
};

function getTabTitle(tab: Tab): string {
  if (tab.title && tab.title !== 'New request') return tab.title;
  if (isRequestTab(tab) && tab.request.url) {
    try { return new URL(tab.request.url).hostname; } catch { return tab.request.url; }
  }
  return 'New request';
}

interface TabItemProps {
  tab: Tab;
  isActive: boolean;
  onSelect: () => void;
  onClose: () => void;
  onDoubleClick?: () => void;
}

export function TabItem({ tab, isActive, onSelect, onClose, onDoubleClick }: TabItemProps) {
  return (
    <div
      role="tab"
      tabIndex={0}
      aria-selected={isActive}
      onClick={onSelect}
      onDoubleClick={onDoubleClick}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onSelect();
        }
      }}
      className={`group flex items-center gap-1.5 px-3 py-2 text-sm border-r border-border/70 cursor-pointer shrink-0 min-w-0 max-w-[190px] transition-all ${
        isActive
          ? 'bg-background/95 border-b-2 border-b-primary -mb-px text-foreground'
          : 'hover:bg-accent/50 text-muted-foreground'
      }`}
    >
      {isRequestTab(tab) ? (
        <span className={`font-semibold text-2xs shrink-0 ${METHOD_TEXT_COLORS[tab.request.method]}`}>
          {tab.request.method}
        </span>
      ) : isGitTab(tab) ? (
        <GitBranch className="h-3 w-3 shrink-0 text-muted-foreground" />
      ) : (
        <Folder className="h-3 w-3 shrink-0 text-primary" />
      )}
      <span className="truncate">{getTabTitle(tab)}</span>
      {tab.isDirty && (
        <span className="text-primary shrink-0 text-2xs" aria-label="Unsaved changes">
          ●
        </span>
      )}
      <button
        type="button"
        aria-label="Close tab"
        onClick={(e) => { e.stopPropagation(); onClose(); }}
        className="shrink-0 opacity-0 group-hover:opacity-100 hover:text-foreground rounded-sm p-0.5 transition-opacity"
      >
        <X className="h-3 w-3" />
      </button>
    </div>
  );
}
