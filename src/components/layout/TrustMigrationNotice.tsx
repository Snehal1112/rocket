import { useEffect, useState } from 'react';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import {
  dismissTrustMigrationNotice,
  getTrustMigrationNotice,
  type TrustMigrationEntry,
} from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';

const CAPABILITY_LABELS: Record<string, string> = {
  developerMode: 'Developer mode',
  contextRoots: 'extra script folders',
  agentRun: 'agent runs',
};

function describe(entry: TrustMigrationEntry): string {
  const labels = entry.capabilities.map((c) => CAPABILITY_LABELS[c] ?? c);
  return `${entry.name} (${labels.join(', ')})`;
}

/**
 * One-time notice after the upgrade that introduced the collection trust gate. Lists the
 * collections that kept extra access. It shows until the user dismisses it with OK.
 */
export function TrustMigrationNotice() {
  const [entries, setEntries] = useState<TrustMigrationEntry[]>([]);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void getTrustMigrationNotice()
      .then((notice) => {
        if (cancelled || notice.collections.length === 0) return;
        setEntries(notice.collections);
        setOpen(true);
      })
      .catch((err) => console.error('[TrustMigrationNotice] load failed', err));
    return () => {
      cancelled = true;
    };
  }, []);

  const dismiss = async () => {
    try {
      await dismissTrustMigrationNotice();
    } catch (err) {
      console.error('[TrustMigrationNotice] dismiss failed', err);
    }
    setOpen(false);
  };

  const review = () => {
    const first = entries[0];
    if (first) usePaneStore.getState().openCollectionTab(first.name, 'overview');
    setOpen(false);
  };

  return (
    <AlertDialog open={open} onOpenChange={setOpen}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Collections now need your permission</AlertDialogTitle>
          <AlertDialogDescription>
            Rocket now asks before a collection gets extra access. These collections already had it
            and keep it: {entries.map(describe).join(', ')}. Review them in each collection&apos;s
            overview.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel onClick={review}>Review</AlertDialogCancel>
          <AlertDialogAction onClick={() => void dismiss()}>OK</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
