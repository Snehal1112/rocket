import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardFooter, CardHeader, CardTitle } from '@/components/ui/card';
import { useAsyncAction } from '@/hooks/useAsyncAction';
import { useNetworkStore } from '@/stores/network-store';
import { useSettingsStore } from '@/stores/settings-store';

/**
 * A one-time card after the first launch: "You're visible on this network as ...".
 * Every button marks the intro as seen. It does not show for a hidden node or when
 * networking is off.
 */
export function NetworkIntroCard() {
  const status = useNetworkStore((s) => s.status);
  const [dismissed, setDismissed] = useState(false);
  const hide = useAsyncAction();

  if (status?.state !== 'running' || status.introSeen || status.hidden || dismissed) {
    return null;
  }

  // The card goes at once. A failed save only means it may return after the next launch.
  async function markSeen() {
    setDismissed(true);
    try {
      await useNetworkStore.getState().markIntroSeen();
    } catch (err) {
      console.error('[NetworkIntroCard] could not save the intro flag', err);
    }
  }

  function editName() {
    useSettingsStore.getState().openSettings('network');
    void markSeen();
  }

  // A failed hide keeps the card. The toast says why.
  async function hideNode() {
    const hidden = await hide.run(() => useNetworkStore.getState().setHidden(true));
    if (hidden) await markSeen();
  }

  return (
    <Card
      role='region'
      aria-label='Team network'
      className='fixed bottom-9 left-3 z-40 w-80 max-w-[calc(100vw-1.5rem)]'
    >
      <CardHeader className='p-4 pb-2'>
        <CardTitle className='text-sm'>Team network</CardTitle>
      </CardHeader>
      <CardContent className='px-4 pb-2 text-sm text-muted-foreground'>
        {`You're visible on this network as '${status.displayName}'.`}
      </CardContent>
      <CardFooter className='flex flex-wrap gap-2 p-4 pt-2'>
        <Button size='sm' variant='outline' onClick={editName}>
          Edit name
        </Button>
        <Button size='sm' variant='outline' onClick={() => void hideNode()}>
          Hide my node
        </Button>
        <Button size='sm' onClick={() => void markSeen()}>
          OK
        </Button>
      </CardFooter>
    </Card>
  );
}
