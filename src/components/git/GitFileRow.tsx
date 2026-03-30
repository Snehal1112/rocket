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
      className="group flex items-center gap-1.5 px-2 py-[3px] hover:bg-muted/50 cursor-pointer min-w-0"
      onClick={onClick}
    >
      <GitStatusBadge status={file.status} />

      {/* Filename — monospace, primary */}
      <span className="truncate font-mono text-[13px] min-w-0 leading-snug">
        {fileName}
      </span>

      {/* Folder path — secondary, muted */}
      {folder && (
        <span className="shrink-0 text-[11px] text-muted-foreground/60 truncate max-w-[35%]">
          {folder}
        </span>
      )}

      {/* Action buttons — visible on hover */}
      <div className="ml-auto shrink-0 flex gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
        <TooltipProvider delayDuration={300}>
          {file.staged && onUnstage && (
            <Tooltip>
              <TooltipTrigger asChild>
                <Button variant="ghost" size="icon" className="h-5 w-5" onClick={(e) => { e.stopPropagation(); onUnstage(); }}>
                  <Minus className="h-3 w-3" />
                </Button>
              </TooltipTrigger>
              <TooltipContent side="left"><p>Unstage</p></TooltipContent>
            </Tooltip>
          )}
          {!file.staged && onStage && (
            <Tooltip>
              <TooltipTrigger asChild>
                <Button variant="ghost" size="icon" className="h-5 w-5" onClick={(e) => { e.stopPropagation(); onStage(); }}>
                  <Plus className="h-3 w-3" />
                </Button>
              </TooltipTrigger>
              <TooltipContent side="left"><p>Stage</p></TooltipContent>
            </Tooltip>
          )}
          {!file.staged && onDiscard && (
            <Tooltip>
              <TooltipTrigger asChild>
                <Button variant="ghost" size="icon" className="h-5 w-5 text-destructive hover:text-destructive" onClick={(e) => { e.stopPropagation(); onDiscard(); }}>
                  <Undo2 className="h-3 w-3" />
                </Button>
              </TooltipTrigger>
              <TooltipContent side="left"><p>Discard changes</p></TooltipContent>
            </Tooltip>
          )}
        </TooltipProvider>
      </div>
    </div>
  );
}
