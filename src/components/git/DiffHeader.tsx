import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
import type { GitStatusKind } from '@/lib/tauri-api';
import type { DiffState } from '@/types/pane-types';
import { GitStatusBadge } from './GitStatusBadge';

interface DiffHeaderProps {
  diffState: DiffState;
  onToggleStaged: (isStaged: boolean) => void;
  mode: 'text' | 'visual';
  onModeChange: (mode: 'text' | 'visual') => void;
  canShowVisual: boolean;
  hideStageToggle?: boolean;
}

// Header bar showing file status badge, path, staged/working toggle, and text/visual mode toggle.
export function DiffHeader({
  diffState,
  onToggleStaged,
  mode,
  onModeChange,
  canShowVisual,
  hideStageToggle = false,
}: DiffHeaderProps) {
  return (
    <div className='flex items-center gap-2 border-b px-3 py-1.5'>
      <GitStatusBadge status={diffState.status as GitStatusKind} />
      <span className='font-mono text-xs truncate'>{diffState.filePath}</span>
      <div className='ml-auto flex items-center gap-2'>
        {canShowVisual && (
          <Tabs value={mode} onValueChange={(v) => onModeChange(v as 'text' | 'visual')}>
            <TabsList className='h-6'>
              <TabsTrigger value='text' className='text-xs px-2 py-0.5'>
                Text
              </TabsTrigger>
              <TabsTrigger value='visual' className='text-xs px-2 py-0.5'>
                Visual
              </TabsTrigger>
            </TabsList>
          </Tabs>
        )}
        {!hideStageToggle && (
          <Tabs
            value={diffState.isStaged ? 'staged' : 'working'}
            onValueChange={(v) => onToggleStaged(v === 'staged')}
          >
            <TabsList className='h-6'>
              <TabsTrigger value='working' className='text-xs px-2 py-0.5'>
                Working
              </TabsTrigger>
              <TabsTrigger value='staged' className='text-xs px-2 py-0.5'>
                Staged
              </TabsTrigger>
            </TabsList>
          </Tabs>
        )}
      </div>
    </div>
  );
}
