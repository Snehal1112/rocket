import { Loader2, Send } from 'lucide-react';
import { useState } from 'react';
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
import { endAgentSession, sendAgentPrompt, startAgentSession } from '@/lib/tauri-api';
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
  const failAgentChatMessage = usePaneStore((s) => s.failAgentChatMessage);
  const markAgentSessionEnded = usePaneStore((s) => s.markAgentSessionEnded);
  const clearAgentSession = usePaneStore((s) => s.clearAgentSession);

  const [selectedAgentConfigId, setSelectedAgentConfigId] = useState('');
  const [promptText, setPromptText] = useState('');
  const [startError, setStartError] = useState<string | null>(null);

  // Streaming events (chunk/finished/failed) are routed into the store by
  // the app-lifetime bridge in agent-session-event-bridge.ts, not here, so
  // replies keep landing while this panel is unmounted or showing another tab.

  const handleStart = async () => {
    if (!selectedAgentConfigId || !cwd || !collectionName) return;
    setStartError(null);
    beginAgentSession(tabId, selectedAgentConfigId);
    try {
      const newSessionId = await startAgentSession(selectedAgentConfigId, cwd, collectionName);
      // The store decides whether this session still has an owner. It
      // refuses when the tab was closed or dropped mid-handshake, and then
      // nothing else would ever end this credentialed agent process.
      const applied = activateAgentSession(tabId, newSessionId);
      if (!applied) {
        endAgentSession(newSessionId).catch((err) => {
          console.error('[AgentChatPanel] failed to end orphaned session', err);
        });
      }
    } catch (err) {
      clearAgentSession(tabId);
      setStartError(String(err));
    }
  };

  const handleSend = async () => {
    if (agentSession?.status !== 'active') return;
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
          autoCorrect='on'
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
