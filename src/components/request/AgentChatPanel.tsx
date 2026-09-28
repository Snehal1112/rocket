import type { UnlistenFn } from '@tauri-apps/api/event';
import { Loader2, Send } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { MarkdownRenderer } from '@/components/collections/MarkdownRenderer';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Textarea } from '@/components/ui/textarea';
import { useCollectionPath } from '@/lib/collection-path';
import { useAgentConfigs } from '@/lib/queries/agent-config-queries';
import {
  endAgentSession,
  onAgentSessionChunk,
  onAgentSessionFailed,
  onAgentSessionFinished,
  sendAgentPrompt,
  startAgentSession,
} from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { AgentChatSession } from '@/types/pane-types';

interface AgentChatPanelProps {
  tabId: string;
  collectionName?: string;
  agentSession?: AgentChatSession;
  onInsertCode: (code: string) => void;
}

export function AgentChatPanel({
  tabId,
  collectionName,
  agentSession,
  onInsertCode,
}: AgentChatPanelProps) {
  const { data: agentConfigs = [] } = useAgentConfigs();
  const cwd = useCollectionPath(collectionName);

  const beginAgentSession = usePaneStore((s) => s.beginAgentSession);
  const activateAgentSession = usePaneStore((s) => s.activateAgentSession);
  const appendAgentChatMessage = usePaneStore((s) => s.appendAgentChatMessage);
  const appendAgentChatChunk = usePaneStore((s) => s.appendAgentChatChunk);
  const completeAgentChatMessage = usePaneStore((s) => s.completeAgentChatMessage);
  const failAgentChatMessage = usePaneStore((s) => s.failAgentChatMessage);
  const markAgentSessionEnded = usePaneStore((s) => s.markAgentSessionEnded);
  const clearAgentSession = usePaneStore((s) => s.clearAgentSession);

  const [selectedAgentConfigId, setSelectedAgentConfigId] = useState('');
  const [promptText, setPromptText] = useState('');
  const [startError, setStartError] = useState<string | null>(null);

  // Kept in sync every render so the event handlers below (subscribed only
  // when the session id/status actually changes) always read the latest
  // message list without needing to resubscribe on every chunk.
  const agentSessionRef = useRef(agentSession);
  agentSessionRef.current = agentSession;

  const sessionId = agentSession?.status === 'active' ? agentSession.sessionId : undefined;

  // Tracks whether this component instance is still mounted. Used by
  // handleStart's continuation below: if the tab (and this panel) is
  // closed while startAgentSession is still in flight, activating the
  // resulting session in the store would be a silent no-op (no tab left
  // to own it) and the backend agent process would leak. See the
  // isMountedRef check in handleStart.
  const isMountedRef = useRef(true);
  useEffect(() => {
    isMountedRef.current = true;
    return () => {
      isMountedRef.current = false;
    };
  }, []);

  useEffect(() => {
    if (!sessionId) return;
    let disposed = false;
    const unlistens: UnlistenFn[] = [];

    const findStreamingMessageId = () =>
      agentSessionRef.current?.messages.find((m) => m.streaming)?.id;

    Promise.all([
      onAgentSessionChunk((e) => {
        if (e.session_id !== sessionId) return;
        const messageId = findStreamingMessageId();
        if (messageId) appendAgentChatChunk(tabId, messageId, e.text);
      }),
      onAgentSessionFinished((e) => {
        if (e.session_id !== sessionId) return;
        const messageId = findStreamingMessageId();
        if (messageId) completeAgentChatMessage(tabId, messageId);
      }),
      onAgentSessionFailed((e) => {
        if (e.session_id !== sessionId) return;
        const messageId = findStreamingMessageId();
        if (messageId) failAgentChatMessage(tabId, messageId, e.error);
      }),
    ]).then((fns) => {
      if (disposed) {
        for (const fn of fns) fn();
      } else {
        unlistens.push(...fns);
      }
    });

    return () => {
      disposed = true;
      for (const fn of unlistens) fn();
    };
  }, [sessionId, tabId, appendAgentChatChunk, completeAgentChatMessage, failAgentChatMessage]);

  const handleStart = async () => {
    if (!selectedAgentConfigId || !cwd) return;
    setStartError(null);
    beginAgentSession(tabId, selectedAgentConfigId);
    try {
      const newSessionId = await startAgentSession(selectedAgentConfigId, cwd);
      if (!isMountedRef.current) {
        // The tab closed mid-handshake: there's no tab left to own this
        // session, so end it instead of leaving an orphaned, credentialed
        // agent process running with nothing to ever call endAgentSession.
        endAgentSession(newSessionId).catch((err) => {
          console.error('[AgentChatPanel] failed to end orphaned session', err);
        });
        return;
      }
      activateAgentSession(tabId, newSessionId);
    } catch (err) {
      if (!isMountedRef.current) return;
      clearAgentSession(tabId);
      setStartError(String(err));
    }
  };

  const handleSend = async () => {
    if (!agentSession || agentSession.status !== 'active') return;
    const text = promptText.trim();
    if (!text) return;
    if (agentSession.messages.some((m) => m.streaming)) return;
    setPromptText('');
    appendAgentChatMessage(tabId, { id: crypto.randomUUID(), role: 'user', text });
    const agentMessageId = crypto.randomUUID();
    appendAgentChatMessage(tabId, {
      id: agentMessageId,
      role: 'agent',
      text: '',
      streaming: true,
    });
    try {
      await sendAgentPrompt(agentSession.sessionId, text);
    } catch (err) {
      failAgentChatMessage(tabId, agentMessageId, String(err));
    }
  };

  const handleEnd = async () => {
    if (!agentSession) return;
    try {
      await endAgentSession(agentSession.sessionId);
    } catch (err) {
      console.error('[AgentChatPanel] end_agent_session failed', err);
    } finally {
      markAgentSessionEnded(tabId);
    }
  };

  const isStreaming = agentSession?.messages.some((m) => m.streaming) ?? false;

  if (!agentSession || agentSession.status === 'ended' || agentSession.status === 'error') {
    return (
      <div className='flex w-72 shrink-0 flex-col gap-3 border-l p-3'>
        <div className='text-xs font-semibold text-muted-foreground uppercase tracking-wide'>
          AI Assist
        </div>
        {agentSession?.status === 'error' && agentSession.error && (
          <p className='text-xs text-destructive'>{agentSession.error}</p>
        )}
        <Select value={selectedAgentConfigId} onValueChange={setSelectedAgentConfigId}>
          <SelectTrigger className='h-8 text-sm'>
            <SelectValue placeholder='Select an agent…' />
          </SelectTrigger>
          <SelectContent>
            {agentConfigs.map((c) => (
              <SelectItem key={c.id} value={c.id}>
                {c.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        {startError && <p className='text-xs text-destructive'>{startError}</p>}
        <Button
          size='sm'
          onClick={() => void handleStart()}
          disabled={!selectedAgentConfigId || !cwd}
        >
          Start
        </Button>
      </div>
    );
  }

  if (agentSession.status === 'starting') {
    return (
      <div className='flex w-72 shrink-0 items-center justify-center gap-2 border-l p-3 text-sm text-muted-foreground'>
        <Loader2 className='h-4 w-4 animate-spin' />
        Starting agent…
      </div>
    );
  }

  return (
    <div className='flex w-96 shrink-0 flex-col border-l' id='agent-chat-panel'>
      <div className='flex items-center justify-between border-b px-3 py-2'>
        <span className='text-xs font-semibold text-muted-foreground uppercase tracking-wide'>
          AI Assist
        </span>
        <Button variant='ghost' size='sm' onClick={() => void handleEnd()}>
          End session
        </Button>
      </div>
      <ScrollArea className='flex-1'>
        <div className='flex flex-col gap-3 p-3'>
          {agentSession.messages.map((m) => (
            <div key={m.id} className='text-sm'>
              <div className='mb-1 text-xs font-semibold text-muted-foreground'>
                {m.role === 'user' ? 'You' : 'Agent'}
              </div>
              <MarkdownRenderer
                renderCodeActions={(code) => (
                  <Button size='sm' variant='outline' onClick={() => onInsertCode(code)}>
                    Insert
                  </Button>
                )}
              >
                {m.text}
              </MarkdownRenderer>
              {m.streaming && <Loader2 className='h-3 w-3 animate-spin' />}
            </div>
          ))}
        </div>
      </ScrollArea>
      <div className='flex items-end gap-2 border-t p-2'>
        <Textarea
          value={promptText}
          onChange={(e) => setPromptText(e.target.value)}
          placeholder='Ask the agent…'
          className='min-h-8 flex-1 resize-none text-sm'
          disabled={isStreaming}
        />
        <Button
          size='sm'
          aria-label='Send'
          onClick={() => void handleSend()}
          disabled={isStreaming || !promptText.trim()}
        >
          <Send className='h-3.5 w-3.5' />
        </Button>
      </div>
    </div>
  );
}
