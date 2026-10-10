import { Sparkles } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { selectTurnRunning, useAssistantStore } from '@/stores/assistant-store';

/**
 * Title bar button that shows and hides the docked AI Assistant. A dot on it
 * pulses while the agent works and stays still while a session is open behind
 * a closed panel.
 */
export function AssistantToggleButton() {
  const panelOpen = useAssistantStore((s) => s.panelOpen);
  const openPanel = useAssistantStore((s) => s.openPanel);
  const closePanel = useAssistantStore((s) => s.closePanel);
  const turnRunning = useAssistantStore(selectTurnRunning);
  const sessionActive = useAssistantStore((s) => s.session?.status === 'active');

  const showDot = turnRunning || (sessionActive && !panelOpen);
  const base = panelOpen ? 'Hide AI Assistant' : 'Show AI Assistant';

  return (
    <Button
      variant='ghost'
      size='icon'
      className='relative h-7 w-7'
      aria-label={turnRunning ? 'AI Assistant, agent working' : 'AI Assistant'}
      aria-pressed={panelOpen}
      aria-controls={panelOpen ? 'assistant-panel' : undefined}
      title={turnRunning ? `${base} (agent working)` : base}
      onClick={() => (panelOpen ? closePanel() : openPanel())}
    >
      <Sparkles className='h-4 w-4' aria-hidden='true' />
      {showDot && (
        <span
          data-testid='assistant-activity-dot'
          data-state={turnRunning ? 'working' : 'open'}
          aria-hidden='true'
          className={`pointer-events-none absolute right-1 top-1 h-1.5 w-1.5 rounded-full bg-primary ${
            turnRunning ? 'motion-safe:animate-pulse' : ''
          }`}
        />
      )}
    </Button>
  );
}
