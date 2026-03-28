import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { GitStatusBadge } from './GitStatusBadge';
import type { DiffState } from '@/types/pane-types';
import type { GitStatusKind } from '@/lib/tauri-api';

interface DiffHeaderProps {
  diffState: DiffState;
  onToggleStaged: (isStaged: boolean) => void;
}

// Header bar showing file status badge, path, and staged/working toggle.
export function DiffHeader({ diffState, onToggleStaged }: DiffHeaderProps) {
  return (
    <div className="flex items-center gap-2 border-b px-3 py-1.5">
      <GitStatusBadge status={diffState.status as GitStatusKind} />
      <span className="font-mono text-xs truncate">{diffState.filePath}</span>
      <div className="ml-auto">
        <Tabs
          value={diffState.isStaged ? 'staged' : 'working'}
          onValueChange={(v) => onToggleStaged(v === 'staged')}
        >
          <TabsList className="h-6">
            <TabsTrigger value="working" className="text-xs px-2 py-0.5">Working</TabsTrigger>
            <TabsTrigger value="staged" className="text-xs px-2 py-0.5">Staged</TabsTrigger>
          </TabsList>
        </Tabs>
      </div>
    </div>
  );
}
