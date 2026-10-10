import { ShieldAlert } from 'lucide-react';
import { useId, useState } from 'react';
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
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import { useCollectionTrust, useSetCapability } from '@/lib/queries/collection-trust-queries';

interface AgentAutonomyToggleProps {
  collectionName: string;
  /** Shows the line about reading and proposing. Hide it where a parent already says so. */
  showHint?: boolean;
}

/**
 * Per-collection switch that lets the AI Assistant send this collection's
 * requests. Reading the workspace and proposing changes is always allowed.
 * Rendered once per collection, so every instance needs its own element id.
 */
export function AgentAutonomyToggle({ collectionName, showHint = true }: AgentAutonomyToggleProps) {
  const switchId = useId();
  const { data: trust, isError: loadFailed } = useCollectionTrust(collectionName);
  const setCapability = useSetCapability(collectionName);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const error = loadFailed ? 'Failed to load this setting.' : saveError;
  // The switch shows what applies. The collection file only requests it.
  const enabled = trust ? trust.agentRun.effective : null;
  const requestedNotAllowed = !!trust && trust.agentRun.requested && !trust.agentRun.granted;

  // Running requests is a capability the user allows on this computer. The backend records
  // the grant and updates the collection file, so a plain settings save is not used.
  const save = async (next: boolean) => {
    setSaving(true);
    try {
      await setCapability.mutateAsync({ capability: 'agentRun', enabled: next });
      setSaveError(null);
      setConfirming(false);
    } catch (err) {
      console.error('[AgentAutonomyToggle] save failed', err);
      setSaveError('Failed to save this setting.');
    } finally {
      setSaving(false);
    }
  };

  const handleCheckedChange = (checked: boolean) => {
    // Turning it on is the risky direction, so it asks first.
    if (checked) setConfirming(true);
    else void save(false);
  };

  return (
    <div className='flex flex-col gap-1.5'>
      <div className='flex items-start gap-2'>
        <Switch
          id={switchId}
          checked={enabled === true}
          disabled={enabled === null || saving}
          onCheckedChange={handleCheckedChange}
        />
        <Label htmlFor={switchId} className='text-xs leading-snug'>
          Allow the agent to run requests in this collection
        </Label>
      </div>
      {showHint && (
        <p className='text-xs text-muted-foreground'>
          Reading and proposing changes is always allowed.
        </p>
      )}
      {requestedNotAllowed && (
        <p className='text-xs text-muted-foreground'>
          This collection&apos;s files turn this on. It is off until you allow it on this computer.
        </p>
      )}
      {error && <p className='text-xs text-destructive'>{error}</p>}

      <AlertDialog open={confirming} onOpenChange={setConfirming}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle className='flex items-center gap-2'>
              <ShieldAlert className='h-4 w-4' aria-hidden='true' />
              Let the agent run requests in this collection?
            </AlertDialogTitle>
            <AlertDialogDescription>
              The agent will be able to send this collection&apos;s requests without asking each
              time. A request it sends can reach any public host, so turn this on only for agents
              and collections you trust.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={saving}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              disabled={saving}
              onClick={(event) => {
                // Keep the dialog open until the save succeeds.
                event.preventDefault();
                void save(true);
              }}
            >
              Allow
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
