// src/components/contracts/ContractDiffPane.tsx

import { format } from 'date-fns';
import { AlertCircle, RefreshCw } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { ChangeChip } from '@/components/contracts/ChangeChip';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Skeleton } from '@/components/ui/skeleton';
import { getContractChangelog } from '@/lib/tauri-api';
import type { ChangeKind } from '@/types/contracts';
import { useContractsStore } from '@/stores/contracts/contractsSlice';
import { usePaneStore } from '@/stores/pane-store';

interface ContractDiffPaneProps {
  collectionId: string;
  contractId: string;
}

function changeTypeToKind(changeType: 'changed' | 'added' | 'removed'): ChangeKind {
  if (changeType === 'added') return 'add';
  if (changeType === 'removed') return 'remove';
  return 'modify';
}

function truncate(val: string | null, max = 40): string {
  if (!val) return '—';
  return val.length > max ? `${val.slice(0, max)}…` : val;
}

export function ContractDiffPane({ collectionId, contractId }: ContractDiffPaneProps) {
  const byId = useContractsStore((s) => s.byId);
  const publishContract = useContractsStore((s) => s.publishContract);
  const closeTab = usePaneStore((s) => s.closeTab);
  const activeGroupId = usePaneStore((s) => s.activeGroupId);

  const contract = byId[contractId];
  const [entries, setEntries] = useState<
    Awaited<ReturnType<typeof getContractChangelog>>['entries']
  >([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [accepting, setAccepting] = useState(false);

  const loadChangelog = useCallback(() => {
    setLoading(true);
    setError(null);
    getContractChangelog(collectionId, contractId)
      .then((log) => setEntries(log.entries))
      .catch((err) => setError(String(err)))
      .finally(() => setLoading(false));
  }, [collectionId, contractId]);

  useEffect(() => {
    loadChangelog();
  }, [loadChangelog]);

  const handleAccept = useCallback(async () => {
    setAccepting(true);
    try {
      await publishContract(collectionId, contractId);
      closeTab(`contract_diff:${contractId}`, activeGroupId);
    } catch (err) {
      console.error('[ContractDiffPane] Accept failed:', err);
    } finally {
      setAccepting(false);
    }
  }, [publishContract, collectionId, contractId, closeTab, activeGroupId]);

  const breakingCount = entries.filter((e) => e.isBreaking).length;
  const canAccept = contract?.status === 'drift' || contract?.status === 'breach';

  return (
    <div className='flex flex-col h-full'>
      {/* Summary bar */}
      <div className='flex items-center gap-3 px-6 py-3 border-b bg-card shrink-0'>
        <span className='font-semibold text-sm truncate'>{contract?.name ?? contractId}</span>
        {contract?.status && (
          <Badge variant='outline' className='capitalize text-xs'>
            {contract.status.replace(/_/g, ' ')}
          </Badge>
        )}
        <span className='text-xs text-muted-foreground ml-auto'>
          {loading ? '…' : `${entries.length} change${entries.length !== 1 ? 's' : ''}`}
          {breakingCount > 0 && (
            <span className='ml-2 text-destructive font-medium'>· {breakingCount} breaking</span>
          )}
        </span>
      </div>

      {/* Content */}
      <ScrollArea className='flex-1 min-h-0'>
        <div className='px-6 py-4'>
          {loading ? (
            <div className='space-y-2'>
              {[1, 2, 3].map((i) => (
                <Skeleton key={i} className='h-10 w-full rounded' />
              ))}
            </div>
          ) : error ? (
            <Card className='border-destructive/30'>
              <CardContent className='flex items-center gap-3 py-4'>
                <AlertCircle className='h-4 w-4 text-destructive shrink-0' />
                <span className='text-sm text-destructive'>{error}</span>
                <Button size='sm' variant='outline' className='ml-auto' onClick={loadChangelog}>
                  <RefreshCw className='h-3.5 w-3.5 mr-1' />
                  Retry
                </Button>
              </CardContent>
            </Card>
          ) : entries.length === 0 ? (
            <p className='text-sm text-muted-foreground py-8 text-center'>
              No changes recorded yet.
            </p>
          ) : (
            <div className='rounded-md border overflow-hidden'>
              <table className='w-full text-sm'>
                <thead className='bg-muted/50'>
                  <tr>
                    <th className='text-left font-medium text-xs text-muted-foreground px-3 py-2 w-32'>
                      Date
                    </th>
                    <th className='text-left font-medium text-xs text-muted-foreground px-3 py-2'>
                      Request
                    </th>
                    <th className='text-left font-medium text-xs text-muted-foreground px-3 py-2 w-24'>
                      Field
                    </th>
                    <th className='text-left font-medium text-xs text-muted-foreground px-3 py-2 w-16'>
                      Kind
                    </th>
                    <th className='text-left font-medium text-xs text-muted-foreground px-3 py-2'>
                      Old → New
                    </th>
                    <th className='text-left font-medium text-xs text-muted-foreground px-3 py-2 w-24'></th>
                  </tr>
                </thead>
                <tbody>
                  {entries.map((entry, idx) => (
                    <tr
                      key={`${entry.requestPath}-${entry.field}-${idx}`}
                      className='border-t hover:bg-muted/20 transition-colors'
                    >
                      <td className='px-3 py-2 text-xs text-muted-foreground whitespace-nowrap'>
                        {format(new Date(entry.timestamp), 'MMM d, HH:mm')}
                      </td>
                      <td className='px-3 py-2 font-mono text-xs truncate max-w-[180px]'>
                        {entry.requestPath}
                      </td>
                      <td className='px-3 py-2 text-xs'>{entry.field}</td>
                      <td className='px-3 py-2'>
                        <ChangeChip kind={changeTypeToKind(entry.changeType)} />
                      </td>
                      <td className='px-3 py-2 text-xs text-muted-foreground font-mono'>
                        <span className='line-through mr-1'>{truncate(entry.oldValue)}</span>
                        <span className='text-foreground'>{truncate(entry.newValue)}</span>
                      </td>
                      <td className='px-3 py-2'>
                        {entry.isBreaking && (
                          <Badge variant='destructive' className='text-[10px] px-1.5 py-0'>
                            Breaking
                          </Badge>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      </ScrollArea>

      {/* Footer CTA */}
      {canAccept && (
        <div className='flex items-center justify-end gap-3 px-6 py-3 border-t bg-card shrink-0'>
          <p className='text-xs text-muted-foreground'>
            Accepting re-signs the contract at the current API shape.
          </p>
          <Button size='sm' disabled={accepting} onClick={() => void handleAccept()}>
            {accepting ? 'Accepting…' : 'Accept all changes'}
          </Button>
        </div>
      )}
    </div>
  );
}
