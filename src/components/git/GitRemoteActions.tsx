import { useState } from 'react';
import { ArrowDown, ArrowUp, RefreshCw, Loader2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { useGitStore } from '@/stores/git-store';

export function GitRemoteActions() {
  const { push, pull, fetch } = useGitStore();
  const [pushing, setPushing] = useState(false);
  const [pulling, setPulling] = useState(false);
  const [fetching, setFetching] = useState(false);

  const handlePush = async () => {
    setPushing(true);
    try { await push(); } finally { setPushing(false); }
  };

  const handlePull = async () => {
    setPulling(true);
    try { await pull(); } finally { setPulling(false); }
  };

  const handleFetch = async () => {
    setFetching(true);
    try { await fetch(); } finally { setFetching(false); }
  };

  return (
    <TooltipProvider delayDuration={300}>
      <div className="flex items-center gap-0.5">
        <Tooltip>
          <TooltipTrigger asChild>
            <Button variant="ghost" size="icon" className="h-6 w-6" onClick={handlePull} disabled={pulling}>
              {pulling ? <Loader2 className="h-3 w-3 animate-spin" /> : <ArrowDown className="h-3 w-3" />}
            </Button>
          </TooltipTrigger>
          <TooltipContent><p>Pull</p></TooltipContent>
        </Tooltip>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button variant="ghost" size="icon" className="h-6 w-6" onClick={handlePush} disabled={pushing}>
              {pushing ? <Loader2 className="h-3 w-3 animate-spin" /> : <ArrowUp className="h-3 w-3" />}
            </Button>
          </TooltipTrigger>
          <TooltipContent><p>Push</p></TooltipContent>
        </Tooltip>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button variant="ghost" size="icon" className="h-6 w-6" onClick={handleFetch} disabled={fetching}>
              {fetching ? <Loader2 className="h-3 w-3 animate-spin" /> : <RefreshCw className="h-3 w-3" />}
            </Button>
          </TooltipTrigger>
          <TooltipContent><p>Fetch</p></TooltipContent>
        </Tooltip>
      </div>
    </TooltipProvider>
  );
}
