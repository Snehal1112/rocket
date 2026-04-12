import { BoxIcon, FileLock, GitBranch, Globe, LayoutDashboard, X } from 'lucide-react';
import { METHOD_TEXT_COLOR } from '@/lib/colors';
import type { Tab } from '@/types/pane-types';
import { isContractTab, isGitTab, isRequestTab, isWorkspaceTab } from '@/types/pane-types';

function getTabTitle(tab: Tab): string {
  if (tab.title && tab.title !== 'New request') return tab.title;
  if (isRequestTab(tab) && tab.request.url) {
    try {
      return new URL(tab.request.url).hostname;
    } catch {
      return tab.request.url;
    }
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
      role='tab'
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
      className={`group flex items-center gap-1.5 px-3 py-2 text-sm border-r border-border/70 cursor-pointer shrink-0 min-w-0 ${isContractTab(tab) ? 'max-w-72' : 'max-w-47.5'} transition-all ${
        isActive
          ? 'bg-background/95 border-b-2 border-b-primary -mb-px text-foreground'
          : 'hover:bg-accent/50 text-muted-foreground'
      }`}
    >
      {isRequestTab(tab) ? (
        <span
          className={`font-semibold text-2xs shrink-0 ${METHOD_TEXT_COLOR[tab.request.method] ?? ''}`}
        >
          {tab.request.method}
        </span>
      ) : isGitTab(tab) ? (
        <GitBranch className='h-3.5 w-3.5 shrink-0 text-muted-foreground' />
      ) : isWorkspaceTab(tab) ? (
        <>
          {tab.activeSection === 'overview' && (
            <LayoutDashboard className='h-3.5 w-3.5 shrink-0 text-muted-foreground' />
          )}
          {tab.activeSection === 'environments' && (
            <Globe className='h-3.5 w-3.5 shrink-0 text-muted-foreground' />
          )}
          {tab.activeSection === 'git' && (
            <GitBranch className='h-3.5 w-3.5 shrink-0 text-muted-foreground' />
          )}
        </>
      ) : isContractTab(tab) ? (
        <FileLock className='h-4 w-4 shrink-0' />
      ) : (
        <BoxIcon className='h-4 w-4 shrink-0' />
      )}
      <span className='truncate'>{getTabTitle(tab)}</span>
      {tab.isDirty && (
        <span
          role='img'
          className='shrink-0 h-1.5 w-1.5 rounded-full bg-amber-500 dark:bg-amber-400'
          aria-label='Unsaved changes'
        />
      )}
      {!isWorkspaceTab(tab) && (
        <button
          type='button'
          aria-label='Close tab'
          onClick={(e) => {
            e.stopPropagation();
            onClose();
          }}
          className='shrink-0 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 hover:text-foreground rounded-sm p-0.5 transition-opacity'
        >
          <X className='h-3.5 w-3.5' />
        </button>
      )}
    </div>
  );
}
