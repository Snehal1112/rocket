import { Check, Save } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import { buildRequestSavePayload } from '@/lib/request-save-mapper';
import { saveRequest } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { RequestTab } from '@/types/pane-types';

interface SaveRequestButtonProps {
  tab: RequestTab;
  groupId: string;
}

export function SaveRequestButton({ tab }: SaveRequestButtonProps) {
  const markClean = usePaneStore((s) => s.markClean);
  const [saveStatus, setSaveStatus] = useState<'idle' | 'success' | 'error'>('idle');

  const handleSave = useCallback(async () => {
    if (!tab.source) return;
    try {
      await saveRequest(tab.source.collection, tab.source.path, buildRequestSavePayload(tab));
      markClean(tab.id);
      setSaveStatus('success');
      setTimeout(() => setSaveStatus('idle'), 2000);
    } catch (err) {
      console.error('[SaveRequestButton] Save failed:', err);
      setSaveStatus('error');
      setTimeout(() => setSaveStatus('idle'), 3000);
    }
  }, [tab, markClean]);

  // Listen for Cmd+S keyboard shortcut.
  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent<{ tabId: string }>).detail;
      if (detail?.tabId !== tab.id) return;
      void handleSave();
    };
    window.addEventListener('rocket:save-draft', handler);
    return () => window.removeEventListener('rocket:save-draft', handler);
  }, [tab.id, handleSave]);

  if (!tab.source) return null;

  return (
    <div className='flex items-center gap-1.5'>
      <Button
        size='sm'
        variant='outline'
        className='h-8 px-3'
        disabled={!tab.isDirty}
        onClick={() => void handleSave()}
      >
        {saveStatus === 'success' ? (
          <Check className='mr-1 h-3.5 w-3.5 text-green-500' />
        ) : (
          <Save className='mr-1 h-3.5 w-3.5' />
        )}
        Save
      </Button>
      {saveStatus === 'error' && <span className='text-2xs text-destructive'>Save failed</span>}
    </div>
  );
}
