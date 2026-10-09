import { Send, Square } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { sendAssistantMessage, stopAssistantTurn } from '@/lib/assistant/assistant-session';
import { selectTurnRunning, useAssistantStore } from '@/stores/assistant-store';

/**
 * Temporary prompt input for the AI Assistant. Plan 06 replaces it with the
 * composer (PromptEditor, chips, pickers). Keep it this small until then.
 */
export function AssistantInputStub() {
  const running = useAssistantStore(selectTurnRunning);
  const [text, setText] = useState('');

  const send = () => {
    if (!text.trim() || running) return;
    const value = text;
    setText('');
    void sendAssistantMessage(value);
  };

  return (
    <div className='flex shrink-0 items-end gap-2 border-t p-2'>
      <Textarea
        aria-label='Message the assistant'
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key !== 'Enter' || e.shiftKey) return;
          // Enter sends here, and must not reach the global send-request shortcut.
          e.preventDefault();
          e.stopPropagation();
          send();
        }}
        placeholder='Ask about this workspace…'
        className='min-h-8 flex-1 resize-none text-sm'
      />
      {running ? (
        <Button
          size='sm'
          variant='outline'
          aria-label='Stop'
          onClick={() => void stopAssistantTurn()}
        >
          <Square className='h-3.5 w-3.5' aria-hidden='true' />
        </Button>
      ) : (
        <Button size='sm' aria-label='Send' disabled={!text.trim()} onClick={send}>
          <Send className='h-3.5 w-3.5' aria-hidden='true' />
        </Button>
      )}
    </div>
  );
}
