import { Info } from 'lucide-react';
import { useEffect, useState } from 'react';
import { getCollectionSettings, type ScriptFlow } from '@/lib/tauri-api';

const HINT_TEXT: Record<ScriptFlow, { script: string; test: string }> = {
  sandwich: {
    script:
      "Folder scripts use the rok API. Folder pre-request scripts run outermost folder first, before the request's. Post-response scripts run after the request's, innermost folder first.",
    test: "Folder scripts use the rok API. Folder tests run after the request's tests, innermost folder first.",
  },
  sequential: {
    script:
      "Folder scripts use the rok API. This collection runs scripts sequentially: in every phase, outermost folder first, then the request's.",
    test: "Folder scripts use the rok API. This collection runs scripts sequentially: folder tests run outermost folder first, then the request's tests.",
  },
};

interface FolderScriptHintProps {
  kind: 'script' | 'test';
  collectionName: string;
}

export function FolderScriptHint({ kind, collectionName }: FolderScriptHintProps) {
  const [flow, setFlow] = useState<ScriptFlow>('sandwich');

  useEffect(() => {
    let cancelled = false;
    getCollectionSettings(collectionName)
      .then((settings) => {
        if (!cancelled) setFlow(settings.scriptFlow === 'sequential' ? 'sequential' : 'sandwich');
      })
      .catch(() => {
        // Keep the sandwich wording.
      });
    return () => {
      cancelled = true;
    };
  }, [collectionName]);

  return (
    <div
      role='note'
      data-testid='folder-script-hint'
      className='flex shrink-0 items-start gap-2 border-t px-3 py-2 text-xs text-muted-foreground'
    >
      <Info className='mt-0.5 h-3.5 w-3.5 shrink-0' aria-hidden='true' />
      <p>{HINT_TEXT[flow][kind]}</p>
    </div>
  );
}
