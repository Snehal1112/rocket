import { useState } from "react";
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
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { useGitStore } from "@/stores/git-store";

export function GitLandingPanel() {
  const { status, push, pull, fetch, saveStash, popStash } = useGitStore();

  const [pushing, setPushing] = useState(false);
  const [pulling, setPulling] = useState(false);
  const [fetching, setFetching] = useState(false);
  const [lastFetched, setLastFetched] = useState<string | null>(null);
  const [showStashDialog, setShowStashDialog] = useState(false);
  const [showFetchFirstDialog, setShowFetchFirstDialog] = useState(false);

  const handleFetch = async () => {
    const { credentials } = useGitStore.getState();
    if (!credentials) {
      // Store will open the credentials dialog; skip timestamp update.
      fetch();
      return;
    }
    setFetching(true);
    try {
      await fetch();
      setLastFetched(new Date().toLocaleTimeString());
    } finally {
      setFetching(false);
    }
  };

  const handlePull = async () => {
    const { credentials } = useGitStore.getState();
    if (!credentials) {
      pull();
      return;
    }

    // Check if working tree has uncommitted changes.
    const { status: currentStatus } = useGitStore.getState();
    if (currentStatus && !currentStatus.isClean) {
      setShowStashDialog(true);
      return;
    }

    setPulling(true);
    try {
      await pull();
    } finally {
      setPulling(false);
    }
  };

  const handleStashAndPull = async () => {
    setShowStashDialog(false);
    setPulling(true);
    try {
      await saveStash("Auto-stash before pull");
      await pull();
      await popStash(0);
    } catch {
      // If pop fails (conflict), stash is preserved for manual resolution.
    } finally {
      setPulling(false);
    }
  };

  const handlePullAnyway = async () => {
    setShowStashDialog(false);
    setPulling(true);
    try {
      await pull();
    } finally {
      setPulling(false);
    }
  };

  const handlePush = async () => {
    const { credentials } = useGitStore.getState();
    if (!credentials) {
      push();
      return;
    }

    // Suggest fetching first if never fetched this session or behind remote.
    const { status: currentStatus } = useGitStore.getState();
    if (!lastFetched || (currentStatus && currentStatus.behind > 0)) {
      setShowFetchFirstDialog(true);
      return;
    }

    setPushing(true);
    try {
      await push();
    } finally {
      setPushing(false);
    }
  };

  const handleFetchAndPush = async () => {
    setShowFetchFirstDialog(false);
    setPushing(true);
    try {
      await fetch();
      setLastFetched(new Date().toLocaleTimeString());
      // Re-check status after fetch — if now behind, abort push.
      const { status: freshStatus } = useGitStore.getState();
      if (freshStatus && freshStatus.behind > 0) {
        setPushing(false);
        return;
      }
      await push();
    } finally {
      setPushing(false);
    }
  };

  const handlePushAnyway = async () => {
    setShowFetchFirstDialog(false);
    setPushing(true);
    try {
      await push();
    } finally {
      setPushing(false);
    }
  };

  const ahead = status?.ahead ?? 0;
  const behind = status?.behind ?? 0;
  const isUpToDate = (status?.isClean ?? false) && ahead === 0 && behind === 0;

  return (
    <div className="flex flex-col items-center justify-center h-full px-6">
      <Card className="w-full max-w-[320px] bg-background/90 shadow-md rounded-md">
        <CardHeader className="flex flex-row items-center px-3 py-2 space-y-0">
          <GitBranch className="h-3.5 w-3.5 text-muted-foreground mr-2" />
          <span className="font-mono text-sm font-medium">
            {status?.branch ?? "no branch"}
          </span>
          <div className="ml-auto flex gap-1.5">
            <Badge
              variant="outline"
              className={ahead > 0 ? "text-amber-500" : "text-emerald-500"}
            >
              ↑{ahead}
            </Badge>
            <Badge
              variant="outline"
              className={behind > 0 ? "text-amber-500" : "text-emerald-500"}
            >
              ↓{behind}
            </Badge>
          </div>
        </CardHeader>

        <CardContent
          className="text-center pb-5 pt-5"
          style={{ backgroundImage: "url(/git-branch.svg)" }}
        >
          {/* Fetch / Pull / Push button group. */}
          <div className="flex gap-2 mb-3">
            <Button
              variant="outline"
              size="sm"
              onClick={handleFetch}
              disabled={fetching}
            >
              {fetching ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <RefreshCw className="h-3.5 w-3.5" />
              )}
              Fetch
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={handlePull}
              disabled={pulling}
            >
              {pulling ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <ArrowDown className="h-3.5 w-3.5" />
              )}
              Pull{behind > 0 ? ` ↓${behind}` : ""}
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={handlePush}
              disabled={pushing}
            >
              {pushing ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <ArrowUp className="h-3.5 w-3.5" />
              )}
              Push{ahead > 0 ? ` ↑${ahead}` : ""}
            </Button>
          </div>

          {/* Last fetched timestamp. */}
          <p className="text-xs text-muted-foreground flex items-center gap-1.5">
            <Clock className="h-3.5 w-3.5 text-muted-foreground" />
            Last fetched:{" "}
            <span className="font-medium text-foreground">
              {lastFetched ?? "Never"}
            </span>
          </p>
        </CardContent>
      </Card>

      {/* Branch status badge. */}
      <div className="flex items-center gap-1.5 text-xs border rounded-md px-3 py-1.5 mt-3">
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

      {/* Auto-stash confirmation dialog. */}
      <AlertDialog open={showStashDialog} onOpenChange={setShowStashDialog}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Uncommitted Changes</AlertDialogTitle>
            <AlertDialogDescription>
              You have uncommitted changes. Pulling may cause conflicts or data
              loss. Would you like to stash your changes first?
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter className="flex-wrap gap-2">
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={handlePullAnyway}>
              Pull Anyway
            </AlertDialogAction>
            <AlertDialogAction onClick={handleStashAndPull}>
              Stash & Pull
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* Fetch-before-push confirmation dialog. */}
      <AlertDialog
        open={showFetchFirstDialog}
        onOpenChange={setShowFetchFirstDialog}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Fetch Before Push</AlertDialogTitle>
            <AlertDialogDescription>
              {(status?.behind ?? 0) > 0
                ? `Your branch is ${status?.behind} commits behind the remote. Fetching first ensures you have the latest changes and reduces the risk of conflicts.`
                : "You have not fetched from the remote yet. Fetching first ensures you have the latest changes and reduces the risk of conflicts."}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter className="flex-wrap gap-2">
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={handlePushAnyway}>
              Push Anyway
            </AlertDialogAction>
            <AlertDialogAction onClick={handleFetchAndPush}>
              Fetch & Push
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
