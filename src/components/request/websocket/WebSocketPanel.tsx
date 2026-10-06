import { Plug, PlugZap } from 'lucide-react';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { SingleLineEditor } from '@/components/editor';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { useWebSocketVariableContext } from '@/hooks/useWebSocketVariableContext';
import { authStateForType } from '@/lib/auth-type-defaults';
import { withCurrentAuthType } from '@/lib/auth-type-options';
import { cn } from '@/lib/utils';
import { parseOptionalMs } from '@/lib/websocket-mapper';
import { createDefaultWebSocketDraft } from '@/lib/websocket-messages';
import { connectTab, disconnectTab, sendSelectedMessage } from '@/lib/websocket-session';
import { usePaneStore } from '@/stores/pane-store';
import { type ConnectionStatus, IDLE_SESSION, useWebSocketStore } from '@/stores/websocket-store';
import type {
  AuthState,
  KeyValueEntry,
  RequestTab,
  WebSocketDraft,
  WebSocketDraftMessage,
} from '@/types/pane-types';
import { AuthEditor } from '../AuthEditor';
import { HeadersEditor } from '../HeadersEditor';
import { SaveRequestButton } from '../SaveRequestButton';
import { SaveToCollectionDialog } from '../SaveToCollectionDialog';
import { MessageLog } from './MessageLog';
import { WebSocketMessagesEditor } from './WebSocketMessagesEditor';

// Only auth types a WebSocket handshake can use are offered; one already set stays selectable.
const AUTH_OPTIONS: { label: string; value: AuthState['authType'] }[] = [
  { label: 'Inherit', value: 'inherit' },
  { label: 'None', value: 'none' },
  { label: 'Basic', value: 'basic' },
  { label: 'Bearer', value: 'bearer' },
  { label: 'API Key', value: 'api-key' },
];

const STATUS_LABEL: Record<ConnectionStatus, string> = {
  idle: 'Disconnected',
  connecting: 'Connecting',
  open: 'Connected',
  closed: 'Closed',
  failed: 'Failed',
};

interface WebSocketPanelProps {
  tab: RequestTab;
  groupId: string;
}

export function WebSocketPanel({ tab, groupId }: WebSocketPanelProps) {
  const updateRequest = usePaneStore((s) => s.updateRequest);
  const session = useWebSocketStore((s) => s.byTab[tab.id]) ?? IDLE_SESSION;
  const clearLog = useWebSocketStore((s) => s.clearLog);
  const variableContext = useWebSocketVariableContext(tab.source?.collection);
  const [saveToCollectionOpen, setSaveToCollectionOpen] = useState(false);

  const request = tab.request;
  const draft = useMemo<WebSocketDraft>(
    () => request.websocket ?? createDefaultWebSocketDraft(),
    [request.websocket],
  );
  const connected = session.status === 'open';
  const connecting = session.status === 'connecting';

  const patchDraft = useCallback(
    (patch: Partial<WebSocketDraft>) =>
      updateRequest(tab.id, { websocket: { ...draft, ...patch } }),
    [draft, tab.id, updateRequest],
  );

  // Ctrl or Cmd+Enter sends the selected message (see useKeyboardShortcuts).
  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent<{ tabId: string }>).detail;
      if (detail?.tabId === tab.id) void sendSelectedMessage(tab);
    };
    window.addEventListener('rocket:websocket-send', handler);
    return () => window.removeEventListener('rocket:websocket-send', handler);
  }, [tab]);

  // Opens the save dialog for an unsaved tab (Ctrl or Cmd+S on a tab with no source).
  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent<{ tabId: string }>).detail;
      if (detail?.tabId === tab.id) setSaveToCollectionOpen(true);
    };
    window.addEventListener('rocket:save-to-collection', handler);
    return () => window.removeEventListener('rocket:save-to-collection', handler);
  }, [tab.id]);

  const authOptions = useMemo(
    () => withCurrentAuthType(AUTH_OPTIONS, request.auth.authType),
    [request.auth.authType],
  );

  return (
    <div className='flex h-full min-h-0 flex-col'>
      <div className='flex items-center gap-2 border-b px-3 py-2'>
        <Badge variant='outline' className='shrink-0'>
          WS
        </Badge>
        <SingleLineEditor
          value={request.url}
          onChange={(url) => updateRequest(tab.id, { url })}
          placeholder='wss://echo.websocket.org'
          variableContext={variableContext}
          onSubmit={() => {
            if (!connected && !connecting) void connectTab(tab);
          }}
          className='flex-1'
        />
        {connected ? (
          <Button
            size='sm'
            variant='outline'
            className='h-8'
            onClick={() => void disconnectTab(tab.id)}
          >
            <PlugZap aria-hidden='true' className='mr-1 h-3.5 w-3.5' /> Disconnect
          </Button>
        ) : (
          <Button
            size='sm'
            className='h-8'
            disabled={connecting || request.url.trim() === ''}
            onClick={() => void connectTab(tab)}
          >
            <Plug aria-hidden='true' className='mr-1 h-3.5 w-3.5' />
            {connecting ? 'Connecting...' : 'Connect'}
          </Button>
        )}
        <Badge
          variant='outline'
          className={cn(
            'shrink-0',
            connected && 'text-[hsl(var(--success))]',
            session.status === 'failed' && 'text-destructive',
          )}
          title={session.error ?? undefined}
        >
          {STATUS_LABEL[session.status]}
        </Badge>
        {!tab.source && (
          <>
            <Button
              size='sm'
              variant='outline'
              className='h-8'
              onClick={() => setSaveToCollectionOpen(true)}
            >
              Save to Collection
            </Button>
            <SaveToCollectionDialog
              open={saveToCollectionOpen}
              tab={tab}
              onClose={() => setSaveToCollectionOpen(false)}
            />
          </>
        )}
        <SaveRequestButton tab={tab} groupId={groupId} />
      </div>

      <div className='flex min-h-0 flex-1 basis-1/2 flex-col px-3 py-2'>
        <Tabs defaultValue='messages' className='flex min-h-0 flex-1 flex-col'>
          <TabsList className='self-start'>
            <TabsTrigger value='messages'>Messages</TabsTrigger>
            <TabsTrigger value='headers'>Headers</TabsTrigger>
            <TabsTrigger value='auth'>Auth</TabsTrigger>
            <TabsTrigger value='settings'>Settings</TabsTrigger>
          </TabsList>
          <TabsContent value='messages' className='min-h-0 flex-1 pt-2'>
            <WebSocketMessagesEditor
              messages={draft.messages}
              onChange={(messages: WebSocketDraftMessage[]) => patchDraft({ messages })}
              onSend={() => void sendSelectedMessage(tab)}
              canSend={connected && draft.messages.length > 0}
              variableContext={variableContext}
            />
          </TabsContent>
          <TabsContent value='headers' className='min-h-0 flex-1 overflow-auto pt-2'>
            <HeadersEditor
              headers={request.headers}
              onChange={(headers: KeyValueEntry[]) => updateRequest(tab.id, { headers })}
              variableContext={variableContext}
            />
          </TabsContent>
          <TabsContent value='auth' className='min-h-0 flex-1 overflow-auto pt-2'>
            <div className='flex flex-col gap-3'>
              <div className='flex flex-col gap-1.5'>
                <Label className='text-xs font-medium'>Auth type</Label>
                <Select
                  value={request.auth.authType}
                  onValueChange={(value) =>
                    updateRequest(tab.id, {
                      auth: authStateForType(value as AuthState['authType'], request.auth),
                    })
                  }
                >
                  <SelectTrigger className='h-8 w-56' aria-label='Auth type'>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {authOptions.map((o) => (
                      <SelectItem key={o.value} value={o.value}>
                        {o.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                <p className='text-xs text-muted-foreground'>
                  OAuth 2, Digest, NTLM, WSSE and AWS Signature are not supported on a WebSocket
                  handshake yet.
                </p>
              </div>
              <AuthEditor
                auth={request.auth}
                onChange={(auth) => updateRequest(tab.id, { auth })}
                variableContext={variableContext}
                collection={tab.source?.collection}
                requestPath={tab.source?.path}
              />
            </div>
          </TabsContent>
          <TabsContent value='settings' className='min-h-0 flex-1 overflow-auto pt-2'>
            <div className='flex max-w-md flex-col gap-3'>
              <div className='flex flex-col gap-1.5'>
                <Label htmlFor='ws-timeout' className='text-xs font-medium'>
                  Connect timeout (ms)
                </Label>
                <Input
                  id='ws-timeout'
                  type='number'
                  min={0}
                  className='h-8'
                  placeholder='inherit (30000), 0 = no timeout'
                  value={draft.timeoutMs === 'inherit' ? '' : draft.timeoutMs}
                  onChange={(e) => patchDraft({ timeoutMs: parseOptionalMs(e.target.value) })}
                />
              </div>
              <div className='flex flex-col gap-1.5'>
                <Label htmlFor='ws-keepalive' className='text-xs font-medium'>
                  Keep-alive ping interval (ms)
                </Label>
                <Input
                  id='ws-keepalive'
                  type='number'
                  min={0}
                  className='h-8'
                  placeholder='inherit (no pings)'
                  value={draft.keepAliveMs === 'inherit' ? '' : draft.keepAliveMs}
                  onChange={(e) => patchDraft({ keepAliveMs: parseOptionalMs(e.target.value) })}
                />
              </div>
              <div className='flex items-center justify-between gap-3'>
                <Label htmlFor='ws-verify' className='text-xs font-medium'>
                  Skip TLS verification (this session only)
                </Label>
                <Switch
                  id='ws-verify'
                  checked={!request.settings.verifySsl}
                  onCheckedChange={(skip) =>
                    updateRequest(tab.id, { settings: { ...request.settings, verifySsl: !skip } })
                  }
                />
              </div>
            </div>
          </TabsContent>
        </Tabs>
      </div>

      <div className='min-h-0 flex-1 basis-1/2 border-t'>
        <MessageLog entries={session.log} onClear={() => clearLog(tab.id)} />
      </div>
    </div>
  );
}
