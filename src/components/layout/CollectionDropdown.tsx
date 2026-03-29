import { useState, useEffect, useCallback } from 'react';
import { ChevronDown, Clock, Briefcase } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { cn } from '@/lib/utils';
import { listCollections, type CollectionSummary } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { useWorkspaceStore } from '@/stores/workspace-store';
import { useGitStore } from '@/stores/git-store';

export function CollectionDropdown() {
  const [open, setOpen] = useState(false);
  const [summaries, setSummaries] = useState<CollectionSummary[]>([]);

  const activeCollection = usePaneStore((s) => s.activeCollection);
  const switchCollection = usePaneStore((s) => s.switchCollection);
  const getOpenTabCount = usePaneStore((s) => s.getOpenTabCount);

  const activeWorkspace = useWorkspaceStore((s) => {
    const ws = s.workspaces.find((w) => w.id === s.activeWorkspaceId);
    return ws?.name ?? 'Untitled Workspace';
  });

  const fetchCollections = useCallback(async () => {
    try {
      const results = await listCollections();
      setSummaries(results);
    } catch (err) {
      console.error('[CollectionDropdown] list error', err);
    }
  }, []);

  useEffect(() => {
    if (open) void fetchCollections();
  }, [open, fetchCollections]);

  const handleSelect = (summary: CollectionSummary) => {
    switchCollection(summary.name);
    useGitStore.getState().setCollection(summary.path);
    setOpen(false);
  };

  const activeTabCount = activeCollection ? getOpenTabCount(activeCollection) : 0;

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="outline"
          size="sm"
          className="h-7 gap-1.5 px-2.5 text-sm font-medium"
        >
          <Clock size={14} className="shrink-0" />
          <span className="max-w-[120px] truncate">{activeCollection ?? 'Select collection'}</span>
          {activeCollection && (
            <span className="text-muted-foreground">{activeTabCount}</span>
          )}
          <ChevronDown className="h-3 w-3 text-muted-foreground" />
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-72 p-0" align="start">
        {/* Workspace section */}
        <div className="px-3 py-2.5 border-b border-border">
          <p className="text-[10px] text-muted-foreground uppercase tracking-wider font-medium mb-1.5">
            Workspace
          </p>
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <Briefcase size={14} className="text-muted-foreground" />
              <span className="text-sm font-medium">{activeWorkspace}</span>
            </div>
            <span className="text-xs text-muted-foreground">{summaries.length}</span>
          </div>
        </div>

        {/* Collections section */}
        <div className="py-1">
          <p className="text-[10px] text-muted-foreground uppercase tracking-wider font-medium px-3 py-1.5">
            Collections
          </p>
          {summaries.length === 0 ? (
            <p className="text-xs text-muted-foreground px-3 py-2">No collections</p>
          ) : (
            summaries.map((summary) => {
              const isActive = summary.name === activeCollection;
              const tabCount = getOpenTabCount(summary.name);
              return (
                <button
                  key={summary.name}
                  type="button"
                  onClick={() => handleSelect(summary)}
                  className={cn(
                    'w-full flex items-center justify-between px-3 py-1.5 text-sm hover:bg-accent/50 transition-colors',
                    isActive && 'bg-accent',
                  )}
                >
                  <div className="flex items-center gap-2 min-w-0">
                    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" className={cn('shrink-0', isActive ? 'text-primary' : 'text-muted-foreground')}>
                      <circle cx="8" cy="8" r="5" stroke="currentColor" strokeWidth="1.2" />
                      <circle cx="8" cy="8" r="2" fill="currentColor" />
                    </svg>
                    <span className={cn('truncate', isActive && 'font-medium')}>{summary.name}</span>
                  </div>
                  {tabCount > 0 ? (
                    <span className={cn(
                      'text-xs px-1.5 rounded-full min-w-[20px] text-center',
                      isActive
                        ? 'bg-primary text-primary-foreground font-semibold'
                        : 'text-muted-foreground',
                    )}>
                      {tabCount}
                    </span>
                  ) : (
                    <span className="text-xs text-muted-foreground">{tabCount}</span>
                  )}
                </button>
              );
            })
          )}
        </div>
      </PopoverContent>
    </Popover>
  );
}
