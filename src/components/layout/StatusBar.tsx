import { Button } from '@/components/ui/button';
import { Plus } from 'lucide-react';
import { usePaneStore } from '@/stores/pane-store';

// Bottom status bar with quick actions.
export function StatusBar() {
  function handleNew() {
    const { activeGroupId, newDraftTab } = usePaneStore.getState();
    newDraftTab(activeGroupId);
  }

  return (
    <div className="h-7 border-t border-border/70 bg-card/85 backdrop-blur-sm px-2 flex items-center gap-1.5 shrink-0">
      <Button
        variant="ghost"
        size="sm"
        className="h-5 px-1.5 text-[11px]"
        onClick={handleNew}
      >
        <Plus className="h-3 w-3 mr-1" />
        New
      </Button>
    </div>
  );
}
