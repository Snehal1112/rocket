import { Plus, Send, Trash2 } from 'lucide-react';
import { lazy, Suspense } from 'react';
import { EditorSkeleton } from '@/components/editor/EditorSkeleton';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { WebSocketMessageKind } from '@/lib/tauri-api';
import type { VariableScopeEntry } from '@/lib/url-variables';
import { cn } from '@/lib/utils';
import {
  addMessage,
  MESSAGE_KINDS,
  removeMessage,
  selectedMessage,
  selectMessage,
  updateMessage,
} from '@/lib/websocket-messages';
import type { WebSocketDraftMessage } from '@/types/pane-types';

// Lazy-load Monaco so it stays out of the initial JS bundle.
const MonacoWrapper = lazy(() =>
  import('@/components/editor/MonacoWrapper').then((m) => ({ default: m.MonacoWrapper })),
);

const LANGUAGE: Record<WebSocketMessageKind, string> = {
  text: 'plaintext',
  json: 'json',
  xml: 'xml',
  binary: 'plaintext',
};

interface WebSocketMessagesEditorProps {
  messages: WebSocketDraftMessage[];
  onChange: (messages: WebSocketDraftMessage[]) => void;
  onSend: () => void;
  canSend: boolean;
  variableContext?: Map<string, VariableScopeEntry>;
}

// The saved messages of a WebSocket request: a list to pick from, and an editor for the
// selected one. Send sends the selected message.
export function WebSocketMessagesEditor({
  messages,
  onChange,
  onSend,
  canSend,
  variableContext,
}: WebSocketMessagesEditorProps) {
  const current = selectedMessage(messages);

  return (
    <div className='flex h-full min-h-0 gap-3'>
      <div className='flex w-44 shrink-0 flex-col gap-1 overflow-auto'>
        {messages.map((m) => (
          <div key={m.id} className='flex items-center gap-1'>
            <Button
              size='sm'
              variant={m.selected ? 'secondary' : 'ghost'}
              className={cn(
                'h-7 flex-1 justify-start truncate px-2 text-xs',
                m.selected && 'font-semibold',
              )}
              aria-label={`Select ${m.title || 'message'}`}
              aria-pressed={m.selected}
              onClick={() => onChange(selectMessage(messages, m.id))}
            >
              {m.title || 'Untitled'}
            </Button>
            <Button
              size='icon'
              variant='ghost'
              className='h-6 w-6'
              aria-label={`Delete ${m.title || 'message'}`}
              onClick={() => onChange(removeMessage(messages, m.id))}
            >
              <Trash2 aria-hidden='true' className='h-3 w-3' />
            </Button>
          </div>
        ))}
        <Button
          size='sm'
          variant='outline'
          className='h-7 justify-start px-2 text-xs'
          aria-label='Add message'
          onClick={() => onChange(addMessage(messages))}
        >
          <Plus aria-hidden='true' className='mr-1 h-3 w-3' /> Add message
        </Button>
      </div>

      {current ? (
        <div className='flex min-h-0 min-w-0 flex-1 flex-col gap-2'>
          <div className='flex items-center gap-2'>
            <Input
              className='h-8 flex-1 text-sm'
              placeholder='Message title'
              aria-label='Message title'
              value={current.title}
              onChange={(e) =>
                onChange(updateMessage(messages, current.id, { title: e.target.value }))
              }
            />
            <Select
              value={current.kind}
              onValueChange={(kind) =>
                onChange(
                  updateMessage(messages, current.id, { kind: kind as WebSocketMessageKind }),
                )
              }
            >
              <SelectTrigger className='h-8 w-40' aria-label='Message type'>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {MESSAGE_KINDS.map((k) => (
                  <SelectItem key={k.value} value={k.value}>
                    {k.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Button
              size='sm'
              className='h-8'
              disabled={!canSend}
              onClick={onSend}
              aria-label='Send'
            >
              <Send aria-hidden='true' className='mr-1 h-3.5 w-3.5' /> Send
            </Button>
          </div>
          <div className='min-h-[140px] flex-1 overflow-hidden rounded-lg border'>
            <Suspense fallback={<EditorSkeleton />}>
              <MonacoWrapper
                value={current.data}
                onChange={(data) => onChange(updateMessage(messages, current.id, { data }))}
                language={LANGUAGE[current.kind]}
                height='100%'
                variableContext={variableContext}
              />
            </Suspense>
          </div>
          {current.kind === 'binary' && (
            <p className='text-xs text-muted-foreground'>
              Binary messages are base64. The log shows what was sent as hex.
            </p>
          )}
        </div>
      ) : (
        <p className='text-sm text-muted-foreground'>
          No saved messages. Add one to start sending.
        </p>
      )}
    </div>
  );
}
