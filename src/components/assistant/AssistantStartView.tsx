import { useState } from 'react';
import { Button } from '@/components/ui/button';
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

/** Agent picker and Start button. Shown before a session and after one ends. */
export function AssistantStartView() {
  const { data: agentConfigs = [] } = useAgentConfigs();
  const error = useAssistantStore((s) =>
    s.session?.status === 'error' ? s.session.error : undefined,
  );
  const hasMessages = useAssistantStore((s) => s.messages.length > 0);
  const [selectedId, setSelectedId] = useState('');
  const agentConfigId = selectedId || agentConfigs[0]?.id || '';

  return (
    <div className={cn('flex shrink-0 flex-col gap-3 p-3', hasMessages && 'border-t')}>
      {!hasMessages && (
        <>
          <p className='text-xs text-muted-foreground'>
            Ask about this workspace, or let the assistant write scripts and tests and organize
            requests. Every change is a proposal you review before anything is written.
          </p>
          <p className='text-xs text-muted-foreground'>
            The assistant signs in with the credentials your agent configuration supplies through
            environment variables (an API key or an OAuth token). A stored <code>claude login</code>{' '}
            is not used.
          </p>
        </>
      )}
      {error && <p className='text-xs text-destructive'>{error}</p>}
      {agentConfigs.length === 0 ? (
        <p className='text-xs text-muted-foreground'>
          No agents are configured yet. Add one with the agent button in the title bar.
        </p>
      ) : (
        <>
          <Select value={agentConfigId} onValueChange={setSelectedId}>
            <SelectTrigger className='h-8 text-sm' aria-label='Agent'>
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
          <Button
            size='sm'
            disabled={!agentConfigId}
            onClick={() => void startAssistant(agentConfigId)}
          >
            {hasMessages ? 'Start a new session' : 'Start'}
          </Button>
        </>
      )}
    </div>
  );
}
