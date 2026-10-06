import { ArrowDown, ArrowUp, Loader2 } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import type { GrpcPair, GrpcStatus } from '@/lib/tauri-api';
import type { GrpcSessionView, GrpcUnaryView } from '@/stores/grpc-store';

interface GrpcResponseViewProps {
  unary?: GrpcUnaryView;
  session?: GrpcSessionView;
}

function StatusBadge({ status, durationMs }: { status: GrpcStatus; durationMs: number }) {
  return (
    <div className='flex items-center gap-2 text-xs'>
      <Badge variant={status.code === 0 ? 'secondary' : 'destructive'}>
        {status.code} {status.codeName}
      </Badge>
      <span className='text-muted-foreground'>{durationMs} ms</span>
      {status.message && <span className='truncate text-muted-foreground'>{status.message}</span>}
    </div>
  );
}

function PairTable({ pairs, empty }: { pairs: GrpcPair[]; empty: string }) {
  if (pairs.length === 0) return <p className='p-3 text-xs text-muted-foreground'>{empty}</p>;
  return (
    <dl className='grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1 p-3 font-mono text-xs'>
      {pairs.map((p, i) => (
        // Metadata may repeat a name, so the position is part of the key.
        // biome-ignore lint/suspicious/noArrayIndexKey: names are not unique.
        <div key={`${p.name}-${i}`} className='contents'>
          <dt className='text-muted-foreground'>{p.name}</dt>
          <dd className='break-all'>{p.value}</dd>
        </div>
      ))}
    </dl>
  );
}

function Json({ text }: { text: string }) {
  return <pre className='whitespace-pre-wrap break-all font-mono text-xs'>{text}</pre>;
}

/** Shows the result of a unary call, or the live log of a streaming call. */
export function GrpcResponseView({ unary, session }: GrpcResponseViewProps) {
  if (session) {
    const done = session.finished;
    return (
      <div className='flex h-full min-h-0 flex-col gap-2'>
        <div className='flex items-center gap-2'>
          {done ? (
            <StatusBadge status={done.status} durationMs={done.durationMs} />
          ) : (
            <span className='flex items-center gap-1.5 text-xs text-muted-foreground'>
              <Loader2 className='h-3 w-3 animate-spin' aria-hidden='true' /> Streaming
            </span>
          )}
        </div>
        <Tabs defaultValue='messages' className='flex min-h-0 flex-1 flex-col'>
          <TabsList className='h-8 self-start'>
            <TabsTrigger value='messages' className='text-xs'>
              Messages ({session.log.length})
            </TabsTrigger>
            <TabsTrigger value='headers' className='text-xs'>
              Headers
            </TabsTrigger>
            <TabsTrigger value='trailers' className='text-xs'>
              Trailers
            </TabsTrigger>
          </TabsList>
          <TabsContent value='messages' className='min-h-0 flex-1'>
            <ScrollArea className='h-full'>
              <ol aria-label='Message log' className='flex flex-col gap-2 p-2'>
                {session.log.map((entry) => (
                  <li key={entry.id} className='rounded-md border border-border/60 p-2'>
                    <div className='mb-1 flex items-center gap-1.5 text-[10px] uppercase text-muted-foreground'>
                      {entry.direction === 'out' ? (
                        <ArrowUp className='h-3 w-3' aria-label='Sent' />
                      ) : (
                        <ArrowDown className='h-3 w-3' aria-label='Received' />
                      )}
                      {entry.direction === 'out' ? 'Sent' : 'Received'}
                      <span>{new Date(entry.at).toLocaleTimeString()}</span>
                    </div>
                    <Json text={entry.json} />
                  </li>
                ))}
                {session.log.length === 0 && (
                  <p className='text-xs text-muted-foreground'>No messages yet.</p>
                )}
              </ol>
            </ScrollArea>
          </TabsContent>
          <TabsContent value='headers'>
            <PairTable pairs={session.headers} empty='No headers yet.' />
          </TabsContent>
          <TabsContent value='trailers'>
            <PairTable pairs={done?.trailers ?? []} empty='Trailers arrive when the call ends.' />
          </TabsContent>
        </Tabs>
      </div>
    );
  }

  if (!unary) {
    return <p className='p-3 text-xs text-muted-foreground'>Send the request to see the reply.</p>;
  }
  if (unary.status === 'sending') {
    return (
      <p className='flex items-center gap-2 p-3 text-xs text-muted-foreground'>
        <Loader2 className='h-3 w-3 animate-spin' aria-hidden='true' /> Sending
      </p>
    );
  }
  if (unary.status === 'error' || !unary.response) {
    return (
      <p role='alert' className='p-3 text-xs text-destructive'>
        {unary.error ?? 'The call failed.'}
      </p>
    );
  }
  const r = unary.response;
  return (
    <div className='flex h-full min-h-0 flex-col gap-2'>
      <StatusBadge status={r.status} durationMs={r.durationMs} />
      <Tabs defaultValue='response' className='flex min-h-0 flex-1 flex-col'>
        <TabsList className='h-8 self-start'>
          <TabsTrigger value='response' className='text-xs'>
            Response
          </TabsTrigger>
          <TabsTrigger value='headers' className='text-xs'>
            Headers
          </TabsTrigger>
          <TabsTrigger value='trailers' className='text-xs'>
            Trailers
          </TabsTrigger>
        </TabsList>
        <TabsContent value='response' className='min-h-0 flex-1'>
          <ScrollArea className='h-full'>
            <div className='p-2'>
              {r.messageJson ? (
                <Json text={r.messageJson} />
              ) : (
                <p className='text-xs text-muted-foreground'>The call returned no message.</p>
              )}
            </div>
          </ScrollArea>
        </TabsContent>
        <TabsContent value='headers'>
          <PairTable pairs={r.headers} empty='No headers.' />
        </TabsContent>
        <TabsContent value='trailers'>
          <PairTable pairs={r.trailers} empty='No trailers.' />
        </TabsContent>
      </Tabs>
    </div>
  );
}
