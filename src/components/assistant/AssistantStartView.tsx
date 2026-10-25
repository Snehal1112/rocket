import {
  FileCode2,
  KeyRound,
  ListTree,
  MessageSquare,
  Play,
  ShieldCheck,
  Sparkles,
} from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { startAssistant } from '@/lib/assistant/assistant-session';
import { useAgentConfigs } from '@/lib/queries/agent-config-queries';
import { cn } from '@/lib/utils';
import { useAssistantStore } from '@/stores/assistant-store';

const CAPABILITIES = [
  { icon: MessageSquare, title: 'Ask', text: 'Questions about your collections and requests.' },
  { icon: FileCode2, title: 'Write', text: 'Scripts and tests for your requests.' },
  { icon: ListTree, title: 'Organize', text: 'Move and group requests into folders.' },
] as const;

/** Agent picker and Start button. Shown before a session and after one ends. */
export function AssistantStartView() {
  const { data: agentConfigs = [] } = useAgentConfigs();
  const error = useAssistantStore((s) =>
    s.session?.status === 'error' ? s.session.error : undefined,
  );
  const hasMessages = useAssistantStore((s) => s.messages.length > 0);
  const startAgain = () => {
    // A failed start has no session id. It retries with its own mode and replay seed, so a
    // continuation survives. A session that died mid-chat restarts with the defaults.
    const state = useAssistantStore.getState();
    const failed =
      state.session?.status === 'error' && !state.session.sessionId ? state.session : undefined;
    const seed =
      failed && state.replay
        ? {
            fromId: state.replay.fromId,
            preface: state.replay.preface,
            ...(state.replay.resume ? { resume: state.replay.resume } : {}),
          }
        : undefined;
    // A conversation that carries on keeps its messages through a retry.
    const seedMessages = seed?.resume ? state.messages : undefined;
    if (seedMessages) void startAssistant(agentConfigId, failed?.mode, seed, seedMessages);
    else void startAssistant(agentConfigId, failed?.mode, seed);
  };
  const [selectedId, setSelectedId] = useState('');
  const agentConfigId = selectedId || agentConfigs[0]?.id || '';

  return (
    <div
      className={cn(
        'flex flex-col gap-5 p-5',
        hasMessages ? 'shrink-0 border-t' : 'min-h-0 flex-1 overflow-y-auto',
      )}
    >
      {!hasMessages && (
        <>
          <div className='flex flex-col gap-3'>
            <div className='flex h-10 w-10 items-center justify-center rounded-xl bg-primary/15 text-primary ring-1 ring-primary/30'>
              <Sparkles className='h-5 w-5' aria-hidden='true' />
            </div>
            <div className='flex flex-col gap-1'>
              <h2 className='text-base font-semibold tracking-tight'>Your workspace assistant</h2>
              <p className='text-sm leading-relaxed text-muted-foreground'>
                Ask about this workspace, or let the assistant write scripts and tests and organize
                requests.
              </p>
            </div>
          </div>
          <ul className='flex flex-col gap-3'>
            {CAPABILITIES.map(({ icon: Icon, title, text }) => (
              <li key={title} className='flex items-start gap-3'>
                <Icon className='mt-0.5 h-4 w-4 shrink-0 text-primary' aria-hidden='true' />
                <div className='flex flex-col'>
                  <span className='text-sm font-medium'>{title}</span>
                  <span className='text-xs leading-relaxed text-muted-foreground'>{text}</span>
                </div>
              </li>
            ))}
          </ul>
          <div className='flex items-start gap-2.5 rounded-lg border border-primary/25 bg-primary/5 px-3 py-2.5'>
            <ShieldCheck className='mt-0.5 h-4 w-4 shrink-0 text-primary' aria-hidden='true' />
            <p className='text-xs leading-relaxed text-foreground/90'>
              Every change is a proposal you review before anything is written.
            </p>
          </div>
        </>
      )}
      {error && (
        <p
          role='alert'
          className='max-h-40 select-text overflow-y-auto whitespace-pre-wrap break-words rounded-md border border-destructive/30 bg-destructive/5 p-2.5 font-mono text-xs text-destructive'
        >
          {error}
        </p>
      )}
      {agentConfigs.length === 0 ? (
        <p className='text-xs text-muted-foreground'>
          No agents are configured yet. Add one with the agent button in the title bar.
        </p>
      ) : (
        <div className={cn('flex flex-col gap-3', !hasMessages && 'mt-auto')}>
          <div className='flex flex-col gap-1.5'>
            <Label htmlFor='assistant-agent' className='text-xs text-muted-foreground'>
              Agent
            </Label>
            <Select value={agentConfigId} onValueChange={setSelectedId}>
              <SelectTrigger id='assistant-agent' className='h-9 text-sm' aria-label='Agent'>
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
          </div>
          <Button className='h-9 gap-2' disabled={!agentConfigId} onClick={startAgain}>
            <Play className='h-3.5 w-3.5' aria-hidden='true' />
            {hasMessages ? 'Start a new session' : 'Start'}
          </Button>
          {!hasMessages && (
            <p className='flex items-start gap-2 text-xs leading-relaxed text-muted-foreground'>
              <KeyRound className='mt-0.5 h-3.5 w-3.5 shrink-0' aria-hidden='true' />
              <span>
                Signs in with the credentials your agent configuration supplies through environment
                variables (an API key or an OAuth token). A stored <code>claude login</code> is not
                used.
              </span>
            </p>
          )}
        </div>
      )}
    </div>
  );
}
