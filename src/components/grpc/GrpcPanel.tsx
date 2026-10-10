import { Check, Play, Send, Square } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { SingleLineEditor } from '@/components/editor';
import { AuthEditor } from '@/components/request/AuthEditor';
import { KeyValueEditor } from '@/components/request/KeyValueEditor';
import { RequestVariablesPanel } from '@/components/request/RequestVariablesPanel';
import { SaveRequestButton } from '@/components/request/SaveRequestButton';
import { SaveToCollectionDialog } from '@/components/request/SaveToCollectionDialog';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { useGrpcVariableContext } from '@/hooks/useGrpcVariableContext';
import { createDefaultGrpcState, DEFAULT_GRPC_MESSAGE } from '@/lib/pane-utils';
import { warnIfProcessEnvWithheld } from '@/lib/process-env-gate';
import { toApiGrpcRequest } from '@/lib/request-save-mapper';
import {
  type GrpcExecuteInput,
  type GrpcMethodInfo,
  grpcEndRequests,
  grpcSendMessage,
  grpcStartSession,
  grpcUnaryCall,
} from '@/lib/tauri-api';
import { ensureGrpcListeners, useGrpcStore } from '@/stores/grpc-store';
import { usePaneStore } from '@/stores/pane-store';
import type { GrpcState, RequestTab } from '@/types/pane-types';
import { GrpcMessageEditor } from './GrpcMessageEditor';
import { GrpcMethodPicker } from './GrpcMethodPicker';
import { GrpcResponseView } from './GrpcResponseView';

/** A unary call that never answers would hang the tab, since it has no Cancel. */
const UNARY_DEADLINE_MS = 30_000;

type Section = 'message' | 'metadata' | 'auth' | 'variables';

interface GrpcPanelProps {
  tab: RequestTab;
  groupId: string;
}

function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** The request tab of a gRPC request: method, message, metadata, auth, and the reply or stream. */
export function GrpcPanel({ tab, groupId }: GrpcPanelProps) {
  const request = tab.request;
  const grpc = request.grpc ?? createDefaultGrpcState();
  const updateRequest = usePaneStore((s) => s.updateRequest);

  const sessionId = useGrpcStore((s) => s.sessionByTab[tab.id]);
  const session = useGrpcStore((s) => (sessionId ? s.sessions[sessionId] : undefined));
  const unary = useGrpcStore((s) => s.unaryByTab[tab.id]);
  const attachSession = useGrpcStore((s) => s.attachSession);
  const detachSession = useGrpcStore((s) => s.detachSession);
  const recordOutbound = useGrpcStore((s) => s.recordOutbound);
  const setUnary = useGrpcStore((s) => s.setUnary);
  const cancelTabSession = useGrpcStore((s) => s.cancelTabSession);

  const scope = useGrpcVariableContext(tab.source);
  const [section, setSection] = useState<Section>('message');
  const [saveOpen, setSaveOpen] = useState(false);
  const [error, setError] = useState('');
  const [endedFor, setEndedFor] = useState<string | null>(null);

  useEffect(() => {
    void ensureGrpcListeners();
  }, []);

  const patchGrpc = useCallback(
    (patch: Partial<GrpcState>) => updateRequest(tab.id, { grpc: { ...grpc, ...patch } }),
    [updateRequest, tab.id, grpc],
  );

  const buildInput = useCallback(
    (): GrpcExecuteInput => ({
      collection: tab.source?.collection,
      request: toApiGrpcRequest(tab.id, tab.title, request),
      message: grpc.messages[grpc.activeMessage]?.content,
      environmentName: scope.environmentName,
      globalEnvName: scope.globalEnvName,
      requestPath: tab.source?.path,
    }),
    [tab.id, tab.title, tab.source, request, grpc.messages, grpc.activeMessage, scope],
  );

  const running = session?.status === 'running';
  const sending = unary?.status === 'sending';
  // Once a call is open, the server's call shape wins over the stored one.
  const liveType = session?.methodType ? session.methodType : grpc.methodType;
  const streamsRequests = liveType === 'client-streaming' || liveType === 'bidi-streaming';
  const requestsEnded = session !== undefined && endedFor === session.id;

  const handlePick = (method: GrpcMethodInfo) =>
    patchGrpc({ method: method.fullName, methodType: method.methodType });

  const handleStart = async () => {
    setError('');
    if (!grpc.method) {
      setError('Choose a method first.');
      return;
    }
    if (grpc.methodType === 'unary') {
      setUnary(tab.id, { status: 'sending' });
      try {
        const input = buildInput();
        await warnIfProcessEnvWithheld(input.collection, [input], tab.title);
        const response = await grpcUnaryCall({ ...input, timeoutMs: UNARY_DEADLINE_MS });
        setUnary(tab.id, { status: 'done', response });
      } catch (err) {
        setUnary(tab.id, { status: 'error', error: errorText(err) });
      }
      return;
    }
    setUnary(tab.id, undefined);
    // The id is ours, so it is attached before the call opens: events that arrive early are
    // kept, and Cancel works while the connection is still being made.
    const id = crypto.randomUUID();
    attachSession(tab.id, id);
    try {
      const input = buildInput();
      await warnIfProcessEnvWithheld(input.collection, [input], tab.title);
      await grpcStartSession(input, id);
    } catch (err) {
      // A cancel during the connect already ended the session. Anything else never opened.
      if (useGrpcStore.getState().sessions[id]?.status === 'finished') return;
      detachSession(tab.id, id);
      setError(errorText(err));
    }
  };

  const handleSendMessage = async () => {
    if (!session) return;
    setError('');
    const content = grpc.messages[grpc.activeMessage]?.content ?? DEFAULT_GRPC_MESSAGE;
    try {
      await grpcSendMessage(session.id, content);
      recordOutbound(session.id, content);
    } catch (err) {
      setError(errorText(err));
    }
  };

  const handleEndRequests = async () => {
    if (!session) return;
    setError('');
    try {
      await grpcEndRequests(session.id);
      setEndedFor(session.id);
    } catch (err) {
      setError(errorText(err));
    }
  };

  const changeMessage = (index: number, patch: { title?: string; content?: string }) =>
    patchGrpc({ messages: grpc.messages.map((m, i) => (i === index ? { ...m, ...patch } : m)) });

  const addMessage = () =>
    patchGrpc({
      messages: [
        ...grpc.messages,
        { id: crypto.randomUUID(), title: '', content: DEFAULT_GRPC_MESSAGE },
      ],
      activeMessage: grpc.messages.length,
    });

  const removeMessage = (index: number) => {
    if (grpc.messages.length <= 1) return;
    const remaining = grpc.messages.filter((_, i) => i !== index);
    const next = index < grpc.activeMessage ? grpc.activeMessage - 1 : grpc.activeMessage;
    patchGrpc({
      messages: remaining,
      activeMessage: Math.max(0, Math.min(next, remaining.length - 1)),
    });
  };

  const startLabel = grpc.methodType === 'unary' ? 'Send' : 'Start';

  return (
    <div className='flex h-full min-h-0 flex-col'>
      <div className='flex items-center gap-2 border-b border-border/60 px-3 py-2'>
        <Badge variant='outline' className='shrink-0 text-teal-500'>
          gRPC
        </Badge>
        <div className='min-w-0 flex-1'>
          <SingleLineEditor
            aria-label='gRPC URL'
            placeholder='localhost:50051 or grpcs://host:443'
            value={request.url}
            onChange={(url) => updateRequest(tab.id, { url })}
            variableContext={scope.variableContext}
          />
        </div>
        {running ? (
          <Button
            size='sm'
            variant='destructive'
            className='h-8'
            onClick={() => void cancelTabSession(tab.id)}
          >
            <Square className='mr-1 h-3.5 w-3.5' aria-hidden='true' /> Cancel
          </Button>
        ) : (
          <Button size='sm' className='h-8' disabled={sending} onClick={() => void handleStart()}>
            {grpc.methodType === 'unary' ? (
              <Send className='mr-1 h-3.5 w-3.5' aria-hidden='true' />
            ) : (
              <Play className='mr-1 h-3.5 w-3.5' aria-hidden='true' />
            )}
            {startLabel}
          </Button>
        )}
        {!tab.source && (
          <>
            <Button size='sm' variant='outline' className='h-8' onClick={() => setSaveOpen(true)}>
              Save to Collection
            </Button>
            <SaveToCollectionDialog open={saveOpen} tab={tab} onClose={() => setSaveOpen(false)} />
          </>
        )}
        <SaveRequestButton tab={tab} groupId={groupId} />
      </div>

      <div className='border-b border-border/60 px-3 py-2'>
        <GrpcMethodPicker
          method={grpc.method}
          methodType={grpc.methodType}
          protoFilePath={grpc.protoFilePath}
          onProtoFilePathChange={(protoFilePath) => patchGrpc({ protoFilePath })}
          onPick={handlePick}
          buildInput={buildInput}
          sourceKey={`${grpc.protoFilePath}\n${request.url}`}
          variableContext={scope.variableContext}
        />
        {error && (
          <p role='alert' className='mt-2 text-xs text-destructive'>
            {error}
          </p>
        )}
      </div>

      <div className='flex min-h-0 flex-1 flex-col gap-2 p-3'>
        <Tabs
          value={section}
          onValueChange={(v) => setSection(v as Section)}
          className='flex min-h-0 flex-1 flex-col'
        >
          <div className='flex items-center justify-between gap-2'>
            <TabsList className='h-8'>
              <TabsTrigger value='message' className='text-xs'>
                Message
              </TabsTrigger>
              <TabsTrigger value='metadata' className='text-xs'>
                Metadata
              </TabsTrigger>
              <TabsTrigger value='auth' className='text-xs'>
                Auth
              </TabsTrigger>
              <TabsTrigger value='variables' className='text-xs'>
                Variables
              </TabsTrigger>
            </TabsList>
            {running && streamsRequests && (
              <div className='flex items-center gap-2'>
                <Button
                  size='sm'
                  variant='outline'
                  className='h-8'
                  disabled={requestsEnded}
                  onClick={() => void handleSendMessage()}
                >
                  <Send className='mr-1 h-3.5 w-3.5' aria-hidden='true' /> Send message
                </Button>
                <Button
                  size='sm'
                  variant='outline'
                  className='h-8'
                  disabled={requestsEnded}
                  onClick={() => void handleEndRequests()}
                >
                  <Check className='mr-1 h-3.5 w-3.5' aria-hidden='true' /> End requests
                </Button>
              </div>
            )}
          </div>
          <TabsContent value='message' className='min-h-0 flex-1'>
            <GrpcMessageEditor
              messages={grpc.messages}
              active={grpc.activeMessage}
              onSelect={(activeMessage) => patchGrpc({ activeMessage })}
              onChange={changeMessage}
              onAdd={addMessage}
              onRemove={removeMessage}
              variableContext={scope.variableContext}
            />
          </TabsContent>
          <TabsContent value='metadata'>
            <KeyValueEditor
              entries={request.headers}
              onChange={(headers) => updateRequest(tab.id, { headers })}
              keyPlaceholder='Metadata name'
              valuePlaceholder='Value'
              addLabel='Add Metadata'
              variableContext={scope.variableContext}
            />
          </TabsContent>
          <TabsContent value='auth'>
            <AuthEditor
              auth={request.auth}
              onChange={(auth) => updateRequest(tab.id, { auth })}
              variableContext={scope.variableContext}
              collection={tab.source?.collection}
              environmentName={scope.environmentName}
              requestPath={tab.source?.path}
            />
          </TabsContent>
          <TabsContent value='variables'>
            {tab.source ? (
              <RequestVariablesPanel
                collection={tab.source.collection}
                requestPath={tab.source.path}
              />
            ) : (
              <p className='p-3 text-xs text-muted-foreground'>
                Save the request to a collection to add request variables.
              </p>
            )}
          </TabsContent>
        </Tabs>
        <div className='min-h-[160px] flex-1 overflow-hidden rounded-md border border-border/60'>
          <GrpcResponseView unary={unary} session={session} />
        </div>
      </div>
    </div>
  );
}
