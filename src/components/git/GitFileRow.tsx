import { Plus, Minus, Undo2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { GitStatusBadge } from './GitStatusBadge';
import type { FileStatus } from '@/lib/tauri-api';

interface GitFileRowProps {
  file: FileStatus;
  onStage?: () => void;
  onUnstage?: () => void;
  onDiscard?: () => void;
  onClick?: () => void;
}

export function GitFileRow({ file, onStage, onUnstage, onDiscard, onClick }: GitFileRowProps) {
  const parts = file.path.split('/');
  const fileName = parts.pop() ?? file.path;
  const folder = parts.join('/');

  return (
    <div
      className="group flex items-center gap-1.5 rounded px-2 py-0.5 hover:bg-muted/50 cursor-pointer text-sm"
      onClick={onClick}
    >
      <GitStatusBadge status={file.status} />
      <span className="truncate font-mono text-xs">{fileName}</span>
      {folder && (
        <span className="truncate text-xs text-muted-foreground">{folder}</span>
      )}
      <div className="ml-auto flex gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
        {file.staged && onUnstage && (
          <TooltipProvider delayDuration={300}>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button variant="ghost" size="icon" className="h-5 w-5" onClick={(e) => { e.stopPropagation(); onUnstage(); }}>
                  <Minus className="h-3 w-3" />
                </Button>
              </TooltipTrigger>
              <TooltipContent side="left"><p>Unstage</p></TooltipContent>
            </Tooltip>
          </TooltipProvider>
        )}
        {!file.staged && onStage && (
          <TooltipProvider delayDuration={300}>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button variant="ghost" size="icon" className="h-5 w-5" onClick={(e) => { e.stopPropagation(); onStage(); }}>
                  <Plus className="h-3 w-3" />
                </Button>
              </TooltipTrigger>
              <TooltipContent side="left"><p>Stage</p></TooltipContent>
            </Tooltip>
          </TooltipProvider>
        )}
        {!file.staged && onDiscard && (
          <TooltipProvider delayDuration={300}>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button variant="ghost" size="icon" className="h-5 w-5 text-destructive" onClick={(e) => { e.stopPropagation(); onDiscard(); }}>
                  <Undo2 className="h-3 w-3" />
                </Button>
              </TooltipTrigger>
              <TooltipContent side="left"><p>Discard changes</p></TooltipContent>
            </Tooltip>
          </TooltipProvider>
        )}
      </div>
    </div>
  );
}
