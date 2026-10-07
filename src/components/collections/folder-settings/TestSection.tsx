import { lazy, Suspense } from 'react';
import { EditorSkeleton } from '@/components/editor/EditorSkeleton';
import { FolderScriptHint } from './FolderScriptHint';
import type { FolderSectionProps } from './sections';

// Lazy so the Monaco and rok type chain loads only when a Test section opens.
const ScriptsTab = lazy(() =>
  import('@/components/request/ScriptsTab').then((m) => ({ default: m.ScriptsTab })),
);

// An empty script becomes undefined so the save removes it from folder.yml.
const orUnset = (value: string) => (value === '' ? undefined : value);

export function TestSection({
  collectionName,
  folderPath,
  settings,
  onChange,
}: FolderSectionProps) {
  // The key remounts the editor stack per folder so refs and phase state never leak across tabs.
  const editorKey = `folder-test:${collectionName}:${folderPath}`;
  return (
    <div className='flex h-full min-h-0 flex-col'>
      <div className='min-h-0 flex-1 overflow-hidden'>
        <Suspense fallback={<EditorSkeleton />}>
          <ScriptsTab
            key={editorKey}
            tabId={editorKey}
            collectionName={collectionName}
            phases={['tests']}
            agentAssist={false}
            preRequestScript={settings.preRequestScript ?? ''}
            postResponseScript={settings.postResponseScript ?? ''}
            testsScript={settings.testsScript ?? ''}
            onChangePreRequest={(v) => onChange({ preRequestScript: orUnset(v) })}
            onChangePostResponse={(v) => onChange({ postResponseScript: orUnset(v) })}
            onChangeTests={(v) => onChange({ testsScript: orUnset(v) })}
          />
        </Suspense>
      </div>
      <FolderScriptHint kind='test' collectionName={collectionName} />
    </div>
  );
}
