import { Loader2, Sparkles, X } from 'lucide-react';
import type { CSSProperties } from 'react';
import { Button } from '@/components/ui/button';
import { endAssistantSession } from '@/lib/assistant/assistant-session';
import { useAssistantStore } from '@/stores/assistant-store';
import { useLayoutStore } from '@/stores/layout-store';
import { AssistantChatView } from './AssistantChatView';
import { AssistantPermissionsPopover } from './AssistantPermissionsPopover';
import { AssistantStartView } from './AssistantStartView';
import { Composer } from './composer/Composer';

const MIN_WIDTH = 320;
const MAX_WIDTH = 720;

function clampWidth(width: number): number {
  return Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, width));
}

/** The workspace AI Assistant, docked on the right of the main layout. */
export function AssistantPanel() {
  const status = useAssistantStore((s) => s.session?.status);
  const hasMessages = useAssistantStore((s) => s.messages.length > 0);
  const closePanel = useAssistantStore((s) => s.closePanel);
  const width = useLayoutStore((s) => s.assistantPanelWidth);
  const setWidth = useLayoutStore((s) => s.setAssistantPanelWidth);

  return (
    <aside
      id='assistant-panel'
      aria-label='AI Assistant'
      style={{ '--assistant-w': `${width}px` } as CSSProperties}
      className='relative flex w-(--assistant-w) shrink-0 flex-col border-l bg-background'
    >
      {/* biome-ignore lint/a11y/useSemanticElements: <hr role="separator"> is a horizontal rule and cannot be a draggable, focusable resize handle */}
      <div
        role='separator'
        aria-orientation='vertical'
        aria-valuenow={width}
        aria-valuemin={MIN_WIDTH}
        aria-valuemax={MAX_WIDTH}
        aria-label='Resize AI Assistant'
        tabIndex={0}
        className='absolute inset-y-0 -left-1 z-10 w-2 cursor-col-resize hover:bg-primary/30 focus-visible:bg-primary/60 focus-visible:outline-none'
        onPointerDown={(e) => {
          e.preventDefault();
          const startX = e.clientX;
          const startWidth = width;
          // The panel grows as the handle moves left.
          const onMove = (ev: PointerEvent) =>
            setWidth(clampWidth(startWidth - (ev.clientX - startX)));
          const onUp = () => {
            window.removeEventListener('pointermove', onMove);
            window.removeEventListener('pointerup', onUp);
          };
          window.addEventListener('pointermove', onMove);
          window.addEventListener('pointerup', onUp);
        }}
        onKeyDown={(e) => {
          if (e.key === 'ArrowLeft') {
            e.preventDefault();
            setWidth(clampWidth(width + 16));
          } else if (e.key === 'ArrowRight') {
            e.preventDefault();
            setWidth(clampWidth(width - 16));
          }
        }}
      />
      <header className='flex h-10 shrink-0 items-center gap-1 border-b px-3'>
        <Sparkles className='h-4 w-4 text-primary' aria-hidden='true' />
        <span className='flex-1 text-sm font-medium'>AI Assistant</span>
        <AssistantPermissionsPopover />
        {(status === 'active' || status === 'starting') && (
          <Button
            variant='ghost'
            size='sm'
            className='h-7 text-xs'
            onClick={() => void endAssistantSession()}
          >
            End session
          </Button>
        )}
        <Button
          variant='ghost'
          size='icon'
          className='h-7 w-7'
          aria-label='Close AI Assistant'
          onClick={closePanel}
        >
          <X className='h-4 w-4' aria-hidden='true' />
        </Button>
      </header>
      {status === 'starting' ? (
        <div className='flex flex-1 items-center justify-center gap-2 text-sm text-muted-foreground'>
          <Loader2 className='h-4 w-4 animate-spin' aria-hidden='true' />
          Starting the assistant…
        </div>
      ) : status === 'active' ? (
        <>
          <AssistantChatView />
          <Composer />
        </>
      ) : (
        <>
          {hasMessages && <AssistantChatView />}
          <AssistantStartView />
        </>
      )}
    </aside>
  );
}
