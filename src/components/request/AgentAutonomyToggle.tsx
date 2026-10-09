import { ShieldAlert } from 'lucide-react';
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
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import {
  type CollectionSettings,
  getCollectionSettings,
  saveCollectionSettings,
} from '@/lib/tauri-api';

interface AgentAutonomyToggleProps {
  collectionName: string;
}

/**
 * Per-collection opt-in that lets an AI Assist agent use Rocket's tools. The
 * backend attaches the tool server when a session starts, so a change only
 * applies to sessions started after it.
 */
export function AgentAutonomyToggle({ collectionName }: AgentAutonomyToggleProps) {
  // null while the setting is loading, so the switch never shows a guess.
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [confirming, setConfirming] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setEnabled(null);
    setError(null);
    void getCollectionSettings(collectionName)
      .then((loaded) => {
        if (!cancelled) setEnabled(loaded.agentAutonomyEnabled ?? false);
      })
      .catch((err) => {
        console.error('[AgentAutonomyToggle] load failed', err);
        if (!cancelled) setError('Failed to load this setting.');
      });
    return () => {
      cancelled = true;
    };
  }, [collectionName]);

  // saveCollectionSettings replaces the whole settings object on the backend,
  // so read the current settings right before writing and change one field.
  const save = async (next: boolean) => {
    setSaving(true);
    try {
      const current = await getCollectionSettings(collectionName);
      const updated: CollectionSettings = { ...current, agentAutonomyEnabled: next };
      await saveCollectionSettings(collectionName, updated);
      setEnabled(next);
      setError(null);
      setConfirming(false);
    } catch (err) {
      console.error('[AgentAutonomyToggle] save failed', err);
      setError('Failed to save this setting.');
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
          id='agent-autonomy-switch'
          checked={enabled === true}
          disabled={enabled === null || saving}
          onCheckedChange={handleCheckedChange}
        />
        <Label htmlFor='agent-autonomy-switch' className='text-xs leading-snug'>
          Allow this agent to run requests and edit files
        </Label>
      </div>
      <p className='text-xs text-muted-foreground'>
        Applies to sessions you start after changing it.
      </p>
      {error && <p className='text-xs text-destructive'>{error}</p>}

      <AlertDialog open={confirming} onOpenChange={setConfirming}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle className='flex items-center gap-2'>
              <ShieldAlert className='h-4 w-4' aria-hidden='true' />
              Let the agent act in this collection?
            </AlertDialogTitle>
            <AlertDialogDescription>
              The agent will be able to send this collection&apos;s requests, edit its request
              scripts and change non-secret environment variables, without asking each time. A
              request it sends can reach any public host, so turn this on only for agents and
              collections you trust.
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
