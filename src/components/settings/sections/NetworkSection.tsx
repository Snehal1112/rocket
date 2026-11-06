import { type ReactNode, useEffect, useState } from 'react';
import { toast } from 'sonner';
import { ListRow } from '@/components/network/ListRow';
import { NetworkIdentity } from '@/components/network/NetworkIdentity';
import { validateDisplayName } from '@/components/network/network-helpers';
import { SettingsSectionHeader } from '@/components/settings/SettingsSectionHeader';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { StateBlock } from '@/components/ui/state-block';
import { Switch } from '@/components/ui/switch';
import { useAsyncAction } from '@/hooks/useAsyncAction';
import { errorText } from '@/lib/error-text';
import {
  listNetworkAddresses,
  listNetworkBlocked,
  type NetworkAddress,
  type NetworkBlockedDevice,
} from '@/lib/tauri-api';
import { useNetworkStore } from '@/stores/network-store';

function Block({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className='space-y-2 border-b border-border py-4 last:border-b-0'>
      <h3 className='text-sm font-medium'>{title}</h3>
      {children}
    </section>
  );
}

// Settings for the team network: visibility, name, port, blocked devices, saved addresses.
export function NetworkSection() {
  const status = useNetworkStore((s) => s.status);
  const error = useNetworkStore((s) => s.error);
  const displayName = status?.displayName ?? '';
  const [name, setName] = useState(displayName);
  const [blocked, setBlocked] = useState<NetworkBlockedDevice[]>([]);
  const [addresses, setAddresses] = useState<NetworkAddress[]>([]);
  const [listError, setListError] = useState<string | null>(null);
  // Failures of the switch and of the row buttons are toasts. A name error shows under the field.
  const visibility = useAsyncAction();
  const nameSave = useAsyncAction('inline');
  const rowAction = useAsyncAction();

  useEffect(() => {
    void useNetworkStore.getState().refresh();
  }, []);

  // The field follows the stored name after a save or a reload.
  useEffect(() => {
    setName(displayName);
  }, [displayName]);

  useEffect(() => {
    let disposed = false;
    Promise.all([listNetworkBlocked(), listNetworkAddresses()]).then(
      ([blockedList, addressList]) => {
        if (disposed) return;
        setBlocked(blockedList);
        setAddresses(addressList);
      },
      (err) => {
        if (!disposed) setListError(errorText(err));
      },
    );
    return () => {
      disposed = true;
    };
  }, []);

  // The switch shows the stored value, so a failed call leaves it where it was.
  const toggleHidden = (hidden: boolean) =>
    visibility.run(() => useNetworkStore.getState().setHidden(hidden));

  async function saveName() {
    const check = validateDisplayName(name);
    if (!check.ok) {
      nameSave.setError(check.error);
      return;
    }
    const saved = await nameSave.run(() => useNetworkStore.getState().saveName(check.name));
    if (saved) toast.success('Name saved.');
  }

  const unblock = (device: NetworkBlockedDevice) =>
    rowAction.run(async () => {
      await useNetworkStore.getState().unblock(device.deviceId);
      setBlocked((list) => list.filter((d) => d.deviceId !== device.deviceId));
    });

  const removeAddress = (address: NetworkAddress) =>
    rowAction.run(async () => {
      await useNetworkStore.getState().removeAddress(address.host, address.port);
      setAddresses((list) =>
        list.filter((a) => !(a.host === address.host && a.port === address.port)),
      );
    });

  return (
    <div>
      <SettingsSectionHeader
        title='Team network'
        description='Find and pair with other Rocket apps on your network.'
      />
      {!status ? (
        <StateBlock tone={error ? 'error' : 'loading'}>{error ?? 'Loading...'}</StateBlock>
      ) : (
        <>
          {status.state === 'off' && (
            <div role='alert' className='mb-2 rounded-md border border-border p-3 text-sm'>
              {status.offReason ?? 'Team network is off.'}
            </div>
          )}
          {status.warnings.length > 0 && (
            <ul className='mb-2 list-disc space-y-1 pl-5 text-sm text-muted-foreground'>
              {status.warnings.map((warning) => (
                <li key={warning}>{warning}</li>
              ))}
            </ul>
          )}

          <Block title='Visibility'>
            <div className='flex items-center justify-between gap-4'>
              <div className='space-y-1'>
                <Label htmlFor='network-hidden'>Hide my node</Label>
                <p className='text-xs text-muted-foreground'>
                  Others on the network do not see you and cannot send pairing requests. Paired
                  nodes still reach you.
                </p>
              </div>
              <Switch
                id='network-hidden'
                checked={status.hidden}
                onCheckedChange={(next) => void toggleHidden(next)}
              />
            </div>
          </Block>

          <Block title='Name'>
            <Label htmlFor='network-display-name'>Display name</Label>
            <div className='flex items-start gap-2'>
              <div className='flex-1 space-y-1'>
                <Input
                  id='network-display-name'
                  value={name}
                  onChange={(e) => {
                    setName(e.target.value);
                    nameSave.setError(null);
                  }}
                />
                {nameSave.error && <p className='text-xs text-destructive'>{nameSave.error}</p>}
              </div>
              <Button
                disabled={nameSave.busy || name.trim() === displayName}
                onClick={() => void saveName()}
              >
                Save
              </Button>
            </div>
          </Block>

          <Block title='This device'>
            <dl className='grid grid-cols-[8rem_1fr] gap-x-4 gap-y-2 text-sm'>
              <dt className='text-muted-foreground'>Port in use</dt>
              <dd className='select-text font-mono'>{status.port ?? 'Not listening'}</dd>
              <dt className='text-muted-foreground'>Device ID</dt>
              <dd className='select-text font-mono'>{status.deviceShortId ?? '-'}</dd>
            </dl>
          </Block>

          <Block title='Blocked devices'>
            {listError && <StateBlock tone='error'>{listError}</StateBlock>}
            {!listError && blocked.length === 0 && (
              <StateBlock tone='empty'>No blocked devices.</StateBlock>
            )}
            <ul className='space-y-1'>
              {blocked.map((device) => (
                <ListRow key={device.deviceId}>
                  <NetworkIdentity name={device.name} shortId={device.shortId} />
                  <Button
                    size='sm'
                    variant='outline'
                    aria-label={`Unblock ${device.name}`}
                    onClick={() => void unblock(device)}
                  >
                    Unblock
                  </Button>
                </ListRow>
              ))}
            </ul>
          </Block>

          <Block title='Saved addresses'>
            {!listError && addresses.length === 0 && (
              <StateBlock tone='empty'>No saved addresses.</StateBlock>
            )}
            <ul className='space-y-1'>
              {addresses.map((address) => {
                const text = `${address.host}:${address.port}`;
                return (
                  <ListRow key={text} className='text-sm'>
                    <span className='min-w-0 flex-1 truncate font-mono'>{text}</span>
                    <Button
                      size='sm'
                      variant='outline'
                      aria-label={`Remove ${text}`}
                      onClick={() => void removeAddress(address)}
                    >
                      Remove
                    </Button>
                  </ListRow>
                );
              })}
            </ul>
          </Block>
        </>
      )}
    </div>
  );
}
