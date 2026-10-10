import { useState } from 'react';
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
  useCollectionTrust,
  useGrantRequested,
  useRevokeTrust,
  useSetCapability,
} from '@/lib/queries/collection-trust-queries';
import type { CollectionCapability, RequestedCapability } from '@/lib/tauri-api';

const yesNo = (v: boolean) => (v ? 'Yes' : 'No');

interface CollectionTrustSectionProps {
  collection: string;
}

/** Requested versus allowed capabilities of a collection, with allow and revoke actions. */
export function CollectionTrustSection({ collection }: CollectionTrustSectionProps) {
  const { data: trust } = useCollectionTrust(collection);
  const setCapability = useSetCapability(collection);
  const grant = useGrantRequested(collection);
  const revokeAll = useRevokeTrust(collection);

  // The action waiting for the user's confirmation. Allowing anything asks first.
  const [confirm, setConfirm] = useState<{ label: string; run: () => void } | null>(null);

  if (!trust) return null;

  const allowRequested = (cap: RequestedCapability) =>
    grant.mutate({ capabilities: [cap], fingerprint: trust.fingerprint });
  const setCap = (capability: CollectionCapability, enabled: boolean) =>
    setCapability.mutate({ capability, enabled });

  const roots = trust.contextRoots;
  const rootsAllowed = roots.requested.length > 0 && roots.pending.length === 0;

  return (
    <div className='flex flex-col gap-3'>
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
                      label: 'Developer mode',
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
                      label: 'Extra script folders',
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
                      label: 'Agent request runs',
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
                        label: 'Host environment variables',
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
        <Button size='sm' variant='ghost' onClick={() => revokeAll.mutate()}>
          Forget this collection&apos;s permissions
        </Button>
      </div>
      <AlertDialog open={confirm !== null} onOpenChange={(open) => !open && setConfirm(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Allow {confirm?.label} on this computer?</AlertDialogTitle>
            <AlertDialogDescription>
              Only allow this for a collection whose authors you trust.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={() => confirm?.run()}>Allow</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
