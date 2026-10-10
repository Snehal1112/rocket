import { ShieldAlert } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Alert } from '@/components/ui/alert';
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Label } from '@/components/ui/label';
import {
  trustErrorMessage,
  useCollectionTrust,
  useGrantRequested,
} from '@/lib/queries/collection-trust-queries';
import type { CollectionTrust, RequestedCapability } from '@/lib/tauri-api';

interface PendingCapability {
  id: RequestedCapability;
  label: string;
  detail: string;
}

/** The capabilities the collection file asks for and the user has not allowed yet. */
export function pendingCapabilities(trust: CollectionTrust): PendingCapability[] {
  const out: PendingCapability[] = [];
  if (trust.developerMode.requested && !trust.developerMode.granted) {
    out.push({
      id: 'developerMode',
      label: 'Developer mode',
      detail: 'Scripts get file and command access.',
    });
  }
  if (trust.contextRoots.pending.length > 0) {
    out.push({
      id: 'contextRoots',
      label: 'Extra script folders',
      detail: trust.contextRoots.pending.join(', '),
    });
  }
  if (trust.agentRun.requested && !trust.agentRun.granted) {
    out.push({
      id: 'agentRun',
      label: 'Agent request runs',
      detail: 'The AI assistant may send this collection’s requests.',
    });
  }
  return out;
}

const CONSEQUENCE: Record<RequestedCapability, string> = {
  developerMode: 'its scripts run in Safe mode',
  contextRoots: 'its extra script folders are not available',
  agentRun: 'the agent cannot run its requests',
};

/** What the user was shown when Review opened. Later refetches never replace it. */
interface Reviewed {
  fingerprint: string;
  pending: PendingCapability[];
}

interface CollectionTrustBannerProps {
  collection: string;
}

/** Warns when a collection asks for more access than it has on this computer. */
export function CollectionTrustBanner({ collection }: CollectionTrustBannerProps) {
  const { data: trust } = useCollectionTrust(collection);
  const grant = useGrantRequested(collection);
  const [reviewed, setReviewed] = useState<Reviewed | null>(null);
  const [selected, setSelected] = useState<RequestedCapability[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [changedNotice, setChangedNotice] = useState(false);

  // The collection file changed while the dialog was open. Drop the review, so nothing
  // is ever approved with a fingerprint the user did not see.
  const liveFingerprint = trust?.fingerprint;
  useEffect(() => {
    if (reviewed && liveFingerprint !== undefined && liveFingerprint !== reviewed.fingerprint) {
      setReviewed(null);
      setSelected([]);
      setError(null);
      setChangedNotice(true);
    }
  }, [liveFingerprint, reviewed]);

  if (!trust) return null;
  const pending = pendingCapabilities(trust);
  if (!trust.storeError && pending.length === 0 && !changedNotice) return null;

  const openReview = () => {
    setChangedNotice(false);
    setError(null);
    setSelected([]);
    setReviewed({ fingerprint: trust.fingerprint, pending });
  };

  const closeReview = () => {
    setReviewed(null);
    setSelected([]);
  };

  const toggle = (id: RequestedCapability, on: boolean) =>
    setSelected((cur) => (on ? [...cur, id] : cur.filter((c) => c !== id)));

  const allow = async () => {
    if (!reviewed) return;
    try {
      await grant.mutateAsync({ capabilities: selected, fingerprint: reviewed.fingerprint });
      closeReview();
      setError(null);
    } catch (err) {
      console.error('[CollectionTrustBanner] grant failed', err);
      setError(trustErrorMessage(err));
    }
  };

  return (
    <div className='flex flex-col gap-2'>
      {trust.storeError && (
        <Alert variant='destructive'>
          <ShieldAlert className='mt-0.5 h-4 w-4 shrink-0' aria-hidden='true' />
          <p>
            Rocket could not read its trust settings, so every collection runs with no extra access.
            Allowing a permission will reset them.
          </p>
        </Alert>
      )}
      {changedNotice && (
        <Alert variant='destructive'>
          <ShieldAlert className='mt-0.5 h-4 w-4 shrink-0' aria-hidden='true' />
          <p>Settings changed, review again.</p>
        </Alert>
      )}
      {pending.length > 0 && (
        <Alert>
          <ShieldAlert className='mt-0.5 h-4 w-4 shrink-0' aria-hidden='true' />
          <div className='flex flex-1 flex-col gap-1'>
            <p className='font-semibold'>
              This collection asks for more access than it has on this computer.
            </p>
            <p className='text-muted-foreground'>
              It asks for: {pending.map((p) => p.label).join(', ')}. Until you allow{' '}
              {pending.length === 1 ? 'it' : 'them'},{' '}
              {pending.map((p) => CONSEQUENCE[p.id]).join(' and ')}. Only allow this for a
              collection whose authors you trust.
            </p>
            <div>
              <Button size='sm' variant='outline' onClick={openReview}>
                Review...
              </Button>
            </div>
          </div>
        </Alert>
      )}

      <AlertDialog open={reviewed !== null} onOpenChange={(open) => !open && closeReview()}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Allow access for this collection?</AlertDialogTitle>
            <AlertDialogDescription>
              Pick what to allow on this computer. Nothing is selected by default.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <div className='flex flex-col gap-3'>
            {reviewed?.pending.map((p) => (
              <div key={p.id} className='flex items-start gap-2'>
                <Checkbox
                  id={`trust-${p.id}`}
                  checked={selected.includes(p.id)}
                  onCheckedChange={(v) => toggle(p.id, v === true)}
                />
                <Label htmlFor={`trust-${p.id}`} className='flex flex-col items-start gap-0.5'>
                  <span>{p.label}</span>
                  <span className='text-xs font-normal text-muted-foreground'>{p.detail}</span>
                </Label>
              </div>
            ))}
            {error && <p className='text-xs text-destructive'>{error}</p>}
          </div>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <Button
              disabled={selected.length === 0 || grant.isPending}
              onClick={() => void allow()}
            >
              Allow selected
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
