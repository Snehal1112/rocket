import { ArrowUp, ArrowDown } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Separator } from '@/components/ui/separator';
import { BranchSelector } from './BranchSelector';
import { GitRemoteActions } from './GitRemoteActions';
import { GitCredentialsDialog } from './GitCredentialsDialog';
import { useGitStore } from '@/stores/git-store';

export function GitBottomBar() {
  const { isRepo, status, showCredentialsDialog } = useGitStore();

  if (!isRepo || !status) return null;

  return (
    <>
      <div className="flex items-center gap-1 px-2">
        <BranchSelector />
        {(status.ahead > 0 || status.behind > 0) && (
          <>
            <Separator orientation="vertical" className="h-4" />
            {status.ahead > 0 && (
              <Badge variant="outline" className="h-4 gap-0.5 px-1 text-[9px] font-mono">
                <ArrowUp className="h-2.5 w-2.5" />
                {status.ahead}
              </Badge>
            )}
            {status.behind > 0 && (
              <Badge variant="outline" className="h-4 gap-0.5 px-1 text-[9px] font-mono">
                <ArrowDown className="h-2.5 w-2.5" />
                {status.behind}
              </Badge>
            )}
          </>
        )}
        <Separator orientation="vertical" className="h-4" />
        <GitRemoteActions />
      </div>
      {showCredentialsDialog && <GitCredentialsDialog />}
    </>
  );
}
