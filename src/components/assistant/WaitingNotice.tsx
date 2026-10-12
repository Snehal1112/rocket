import { Clock } from 'lucide-react';
import { useEffect, useState } from 'react';
import { selectTurnRunning, useAssistantStore } from '@/stores/assistant-store';

/** Seconds without any agent update before the notice appears. */
const QUIET_AFTER_SECONDS = 20;

/** Tells the user the agent is still working when it has sent nothing for a while. */
export function WaitingNotice() {
  const running = useAssistantStore(selectTurnRunning);
  const lastActivityAt = useAssistantStore((s) => s.lastActivityAt);
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    if (!running) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [running]);

  if (!running || lastActivityAt === undefined) return null;
  const quietFor = Math.floor((now - lastActivityAt) / 1000);
  if (quietFor < QUIET_AFTER_SECONDS) return null;
  return (
    <p className='flex items-center gap-1.5 px-1.5 text-xs text-muted-foreground' role='status'>
      <Clock className='h-3 w-3' />
      Still working: {quietFor}s since the last update. Deep reasoning can take a few minutes. Use
      Stop to cancel.
    </p>
  );
}
