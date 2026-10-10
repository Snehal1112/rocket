import { useState } from 'react';
import { Alert } from '@/components/ui/alert';
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
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import {
  trustErrorMessage,
  useCollectionTrust,
  useGrantRequested,
  useRevokeTrust,
  useSetCapability,
} from '@/lib/queries/collection-trust-queries';
import type { CollectionCapability, RequestedCapability } from '@/lib/tauri-api';

const FILE_NOTE = 'This also updates the collection file.';

const yesNo = (v: boolean) => (v ? 'Yes' : 'No');

interface CollectionTrustSectionProps {
  collection: string;
}

/** Requested versus allowed capabilities of a collection, with allow and revoke actions. */
export function CollectionTrustSection({ collection }: CollectionTrustSectionProps) {
  const { data: trust, isError: loadFailed } = useCollectionTrust(collection);
  const setCapability = useSetCapability(collection);
  const grant = useGrantRequested(collection);
  const revokeAll = useRevokeTrust(collection);

  // The action waiting for the user's confirmation. Allowing anything asks first.
  const [confirm, setConfirm] = useState<{
    title: string;
    note: string;
    confirmLabel?: string;
    run: () => void;
  } | null>(null);

  const failure = loadFailed
    ? 'Could not load the permissions of this collection.'
    : [setCapability.error, grant.error, revokeAll.error]
        .filter((e): e is Error => e !== null)
        .map(trustErrorMessage)[0];

  if (!trust) {
    return failure ? <Alert variant='destructive'>{failure}</Alert> : null;
  }

  const allowRequested = (cap: RequestedCapability) =>
    grant.mutate({ capabilities: [cap], fingerprint: trust.fingerprint });
  const setCap = (capability: CollectionCapability, enabled: boolean) =>
    setCapability.mutate({ capability, enabled });

  const roots = trust.contextRoots;
  const rootsAllowed = roots.requested.length > 0 && roots.pending.length === 0;

  return (
    <div className='flex flex-col gap-3'>
      {failure && <Alert variant='destructive'>{failure}</Alert>}
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Capability</TableHead>
            <TableHead>Requested by the collection</TableHead>
            <TableHead>Allowed on this computer</TableHead>
            <TableHead className='text-right'>Action</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          <TableRow>
            <TableCell>Developer mode (scripts get file and command access)</TableCell>
            <TableCell>{yesNo(trust.developerMode.requested)}</TableCell>
            <TableCell>{yesNo(trust.developerMode.granted)}</TableCell>
            <TableCell className='text-right'>
              {trust.developerMode.granted ? (
                <Button size='sm' variant='outline' onClick={() => setCap('developerMode', false)}>
                  Revoke
                </Button>
              ) : (
                <Button
                  size='sm'
                  variant='outline'
                  onClick={() =>
                    setConfirm({
                      title: 'Allow Developer mode on this computer?',
                      note: FILE_NOTE,
                      run: () =>
                        trust.developerMode.requested
                          ? allowRequested('developerMode')
                          : setCap('developerMode', true),
                    })
                  }
                >
                  Allow...
                </Button>
              )}
            </TableCell>
          </TableRow>
          <TableRow>
            <TableCell>Extra script folders</TableCell>
            <TableCell>
              {roots.requested.length === 0 ? (
                'No'
              ) : (
                <span className='flex flex-wrap items-center gap-1'>
                  {roots.requested.join(', ')}
                  {roots.pending.length > 0 && <Badge variant='warning'>Pending</Badge>}
                </span>
              )}
            </TableCell>
            <TableCell>{roots.granted.length > 0 ? roots.granted.join(', ') : 'No'}</TableCell>
            <TableCell className='text-right'>
              {roots.pending.length > 0 ? (
                <Button
                  size='sm'
                  variant='outline'
                  onClick={() =>
                    setConfirm({
                      title: 'Allow extra script folders on this computer?',
                      note: '',
                      run: () => allowRequested('contextRoots'),
                    })
                  }
                >
                  Allow...
                </Button>
              ) : (
                rootsAllowed && <span className='text-xs text-muted-foreground'>Allowed</span>
              )}
            </TableCell>
          </TableRow>
          <TableRow>
            <TableCell>Agent may run requests</TableCell>
            <TableCell>{yesNo(trust.agentRun.requested)}</TableCell>
            <TableCell>{yesNo(trust.agentRun.granted)}</TableCell>
            <TableCell className='text-right'>
              {trust.agentRun.granted ? (
                <Button size='sm' variant='outline' onClick={() => setCap('agentRun', false)}>
                  Revoke
                </Button>
              ) : (
                <Button
                  size='sm'
                  variant='outline'
                  onClick={() =>
                    setConfirm({
                      title: 'Allow agent request runs on this computer?',
                      note: FILE_NOTE,
                      run: () =>
                        trust.agentRun.requested
                          ? allowRequested('agentRun')
                          : setCap('agentRun', true),
                    })
                  }
                >
                  Allow...
                </Button>
              )}
            </TableCell>
          </TableRow>
          <TableRow>
            <TableCell>Host environment variables ({'{{process.env.*}}'})</TableCell>
            <TableCell>If a request uses it</TableCell>
            <TableCell>{yesNo(trust.processEnv.granted)}</TableCell>
            <TableCell className='text-right'>
              <Button
                size='sm'
                variant='outline'
                onClick={() =>
                  trust.processEnv.granted
                    ? setCap('processEnv', false)
                    : setConfirm({
                        title: 'Allow host environment variables on this computer?',
                        note: '',
                        run: () => setCap('processEnv', true),
                      })
                }
              >
                {trust.processEnv.granted ? 'Revoke' : 'Allow...'}
              </Button>
            </TableCell>
          </TableRow>
        </TableBody>
      </Table>
      <div>
        <Button
          size='sm'
          variant='ghost'
          onClick={() =>
            setConfirm({
              title: 'Forget all permissions of this collection?',
              note: 'This removes every permission above, including host environment access. The collection file is not changed.',
              confirmLabel: 'Forget',
              run: () => revokeAll.mutate(),
            })
          }
        >
          Forget this collection&apos;s permissions
        </Button>
      </div>
      <AlertDialog open={confirm !== null} onOpenChange={(open) => !open && setConfirm(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{confirm?.title}</AlertDialogTitle>
            <AlertDialogDescription>
              Only allow this for a collection whose authors you trust. {confirm?.note}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={() => confirm?.run()}>
              {confirm?.confirmLabel ?? 'Allow'}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
