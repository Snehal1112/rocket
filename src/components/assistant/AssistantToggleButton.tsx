import { Sparkles } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { useAssistantStore } from '@/stores/assistant-store';

/** Title bar button that shows and hides the docked AI Assistant. */
export function AssistantToggleButton() {
  const panelOpen = useAssistantStore((s) => s.panelOpen);
  const openPanel = useAssistantStore((s) => s.openPanel);
  const closePanel = useAssistantStore((s) => s.closePanel);

  return (
    <Button
      variant='ghost'
      size='icon'
      className='h-7 w-7'
      aria-label='AI Assistant'
      aria-pressed={panelOpen}
      aria-controls='assistant-panel'
      title={panelOpen ? 'Hide AI Assistant' : 'Show AI Assistant'}
      onClick={() => (panelOpen ? closePanel() : openPanel())}
    >
      <Sparkles className='h-4 w-4' aria-hidden='true' />
    </Button>
  );
}
