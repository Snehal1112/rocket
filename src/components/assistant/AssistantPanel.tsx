import { History, Loader2, MessageSquare, Power, Sparkles, X } from 'lucide-react';
import { type CSSProperties, type ReactNode, useEffect, useState } from 'react';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { Button } from '@/components/ui/button';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { endAssistantSession } from '@/lib/assistant/assistant-session';
import { useAgentConfigs } from '@/lib/queries/agent-config-queries';
import { useAssistantStore } from '@/stores/assistant-store';
import { useLayoutStore } from '@/stores/layout-store';
import { AssistantChatView } from './AssistantChatView';
import { AssistantHistoryView } from './AssistantHistoryView';
import { AssistantPermissionsPopover } from './AssistantPermissionsPopover';
import { AssistantStartView } from './AssistantStartView';
import { Composer } from './composer/Composer';

const MIN_WIDTH = 320;
const MAX_WIDTH = 720;
/** Width the main content keeps at least, so a narrow window stays usable. */
const MIN_CONTENT_WIDTH = 480;

/** Clamps to the panel limits and to what the window can spare. */
export function clampWidth(width: number, windowWidth: number): number {
  const max = Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, windowWidth - MIN_CONTENT_WIDTH));
  return Math.min(max, Math.max(MIN_WIDTH, width));
}

/** A tooltip for an icon button in the header. */
function HeaderTip({ label, children }: { label: string; children: ReactNode }) {
  return (
    <TooltipProvider delayDuration={300}>
      <Tooltip>
        <TooltipTrigger asChild>{children}</TooltipTrigger>
        <TooltipContent>{label}</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}

/** The workspace AI Assistant, docked on the right of the main layout. */
export function AssistantPanel() {
  const status = useAssistantStore((s) => s.session?.status);
  const agentConfigId = useAssistantStore((s) => s.session?.agentConfigId);
  const { data: agentConfigs } = useAgentConfigs();
  const hasMessages = useAssistantStore((s) => s.messages.length > 0);
  const closePanel = useAssistantStore((s) => s.closePanel);
  const storedWidth = useLayoutStore((s) => s.assistantPanelWidth);
  const setStoredWidth = useLayoutStore((s) => s.setAssistantPanelWidth);
  const [windowWidth, setWindowWidth] = useState(() => window.innerWidth);
  const [showHistory, setShowHistory] = useState(false);
  const [confirmEnd, setConfirmEnd] = useState(false);

  // The stored width stays the user's choice. Only the shown width follows the window.
  useEffect(() => {
    const onResize = () => setWindowWidth(window.innerWidth);
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, []);

  // The agent shows only while a session runs. A removed config gets a neutral name, not its id.
  const running = status === 'active' || status === 'starting';
  const agentLabel =
    running && agentConfigs
      ? (agentConfigs.find((c) => c.id === agentConfigId)?.label ?? 'Agent removed')
      : undefined;

  const width = clampWidth(storedWidth, windowWidth);
  const maxWidth = clampWidth(MAX_WIDTH, windowWidth);
  const setWidth = (w: number) => setStoredWidth(clampWidth(w, window.innerWidth));

  return (
    <aside
      id='assistant-panel'
      aria-label='AI Assistant'
      style={{ '--assistant-w': `${width}px` } as CSSProperties}
      // Above the editor (z-5, like the sidebar) so the raised shadow shows over it and Settings.
      className='elevation-raised-end relative z-[5] flex w-(--assistant-w) shrink-0 flex-col border-l'
    >
      {/* biome-ignore lint/a11y/useSemanticElements: <hr role="separator"> is a horizontal rule and cannot be a draggable, focusable resize handle */}
      <div
        role='separator'
        aria-orientation='vertical'
        aria-valuenow={width}
        aria-valuemin={MIN_WIDTH}
        aria-valuemax={maxWidth}
        aria-label='Resize AI Assistant'
        tabIndex={0}
        className='absolute inset-y-0 -left-1 z-10 w-2 cursor-col-resize hover:bg-primary/30 focus-visible:bg-primary/60 focus-visible:outline-none'
        onPointerDown={(e) => {
          e.preventDefault();
          const startX = e.clientX;
          const startWidth = width;
          // The panel grows as the handle moves left.
          const onMove = (ev: PointerEvent) => setWidth(startWidth - (ev.clientX - startX));
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
            setWidth(width + 16);
          } else if (e.key === 'ArrowRight') {
            e.preventDefault();
            setWidth(width - 16);
          }
        }}
      />
      <header className='flex h-10 shrink-0 items-center gap-1 border-b px-3'>
        <Sparkles className='h-4 w-4 text-primary' aria-hidden='true' />
        <span className='shrink-0 text-sm font-medium'>AI Assistant</span>
        <span className='min-w-0 flex-1 truncate text-xs text-muted-foreground' title={agentLabel}>
          {agentLabel && (
            <>
              <span aria-hidden='true'>· </span>
              {agentLabel}
            </>
          )}
        </span>
        <HeaderTip label={showHistory ? 'Back to the chat' : 'Conversation history'}>
          <Button
            variant='ghost'
            size='icon'
            className='h-7 w-7'
            aria-label={showHistory ? 'Back to the chat' : 'Conversation history'}
            onClick={() => setShowHistory((shown) => !shown)}
          >
            {showHistory ? (
              <MessageSquare className='h-4 w-4' aria-hidden='true' />
            ) : (
              <History className='h-4 w-4' aria-hidden='true' />
            )}
          </Button>
        </HeaderTip>
        <AssistantPermissionsPopover />
        {running && (
          <HeaderTip label='End session'>
            <Button
              variant='ghost'
              size='icon'
              className='h-7 w-7'
              aria-label='End session'
              onClick={() => setConfirmEnd(true)}
            >
              <Power className='h-4 w-4' aria-hidden='true' />
            </Button>
          </HeaderTip>
        )}
        <HeaderTip label='Close AI Assistant'>
          <Button
            variant='ghost'
            size='icon'
            className='h-7 w-7'
            aria-label='Close AI Assistant'
            onClick={closePanel}
          >
            <X className='h-4 w-4' aria-hidden='true' />
          </Button>
        </HeaderTip>
      </header>
      <AlertDialog open={confirmEnd} onOpenChange={setConfirmEnd}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>End this session?</AlertDialogTitle>
            <AlertDialogDescription>
              The conversation is saved to the history. To go on, continue it from there.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Keep session</AlertDialogCancel>
            <AlertDialogAction onClick={() => void endAssistantSession()}>
              End session
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
      {showHistory && (
        <AssistantHistoryView
          onContinued={() => setShowHistory(false)}
          onContinueFailed={() => setShowHistory(true)}
        />
      )}
      {/* The chat stays mounted while the history is shown, so the draft is kept. */}
      <div
        className={showHistory ? 'hidden' : 'flex min-h-0 flex-1 flex-col'}
        aria-hidden={showHistory}
        inert={showHistory}
      >
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
      </div>
    </aside>
  );
}
