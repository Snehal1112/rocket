import { useState } from 'react';
import {
  GitBranch,
  RefreshCw,
  ArrowDown,
  ArrowUp,
  Loader2,
  Clock,
  Check,
  AlertCircle,
  GitCommit,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { useGitStore } from '@/stores/git-store';

export function GitLandingPanel() {
  const { status, push, pull, fetch } = useGitStore();

  const [pushing, setPushing] = useState(false);
  const [pulling, setPulling] = useState(false);
  const [fetching, setFetching] = useState(false);
  const [lastFetched, setLastFetched] = useState<string | null>(null);

  const handleFetch = async () => {
    setFetching(true);
    try {
      await fetch();
      setLastFetched(new Date().toLocaleTimeString());
    } finally {
      setFetching(false);
    }
  };

  const handlePull = async () => {
    setPulling(true);
    try { await pull(); } finally { setPulling(false); }
  };

  const handlePush = async () => {
    setPushing(true);
    try { await push(); } finally { setPushing(false); }
  };

  const ahead = status?.ahead ?? 0;
  const behind = status?.behind ?? 0;
  const isUpToDate = (status?.isClean ?? false) && ahead === 0 && behind === 0;

  return (
    <div className="flex flex-col items-center justify-center h-full px-6">
      <GitBranch className="h-12 w-12 text-muted-foreground/30" />

      <p className="text-sm text-muted-foreground text-center max-w-[280px] mt-4 mb-6">
        Perform git actions or open files from sidebar to view
      </p>

      {/* Fetch / Pull / Push button group. */}
      <div className="flex gap-2 mb-6">
        <Button variant="outline" size="sm" onClick={handleFetch} disabled={fetching}>
          {fetching ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <RefreshCw className="h-3.5 w-3.5" />
          )}
          Fetch
        </Button>
        <Button variant="outline" size="sm" onClick={handlePull} disabled={pulling}>
          {pulling ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <ArrowDown className="h-3.5 w-3.5" />
          )}
          Pull
        </Button>
        <Button variant="outline" size="sm" onClick={handlePush} disabled={pushing}>
          {pushing ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <ArrowUp className="h-3.5 w-3.5" />
          )}
          Push
        </Button>
      </div>

      {/* Last fetched timestamp. */}
      <p className="text-xs text-muted-foreground flex items-center gap-1.5 mb-1.5">
        <Clock className="h-3 w-3" />
        Last fetched:{' '}
        <span className="font-medium text-foreground">{lastFetched ?? 'Never'}</span>
      </p>

      {/* Ahead / behind counts. */}
      <p className="text-xs text-muted-foreground mb-4">
        ↑ {ahead} Ahead&nbsp; |&nbsp; ↓ {behind} Behind
      </p>

      {/* Branch status badge. */}
      <div className="flex items-center gap-1.5 text-xs border rounded-md px-3 py-1.5">
        {isUpToDate ? (
          <>
            <Check className="h-3.5 w-3.5 text-emerald-500" />
            Your branch is up to date
          </>
        ) : behind > 0 ? (
          <>
            <AlertCircle className="h-3.5 w-3.5 text-amber-500" />
            {behind} commits behind
          </>
        ) : (
          <>
            <GitCommit className="h-3.5 w-3.5 text-muted-foreground" />
            {ahead} commits ahead
          </>
        )}
      </div>
    </div>
  );
}
