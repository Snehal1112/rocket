import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { AuthEditor } from '@/components/request/AuthEditor';
import { HeadersEditor } from '@/components/request/HeadersEditor';
import { cn } from '@/lib/utils';
import type { AuthState, KeyValueEntry } from '@/types/pane-types';
import { saveCollectionSettings } from '@/lib/tauri-api';
import { toApiAuth } from '@/lib/execute-request';

interface CollectionSettingsDialogProps {
  collectionName: string;
  onClose: () => void;
}

type SettingsTab = 'auth' | 'headers';

const TABS: { label: string; value: SettingsTab }[] = [
  { label: 'Auth', value: 'auth' },
  { label: 'Headers', value: 'headers' },
];

// Default auth state with no authentication configured.
const DEFAULT_AUTH: AuthState = { authType: 'none' };

export function CollectionSettingsDialog({
  collectionName,
  onClose,
}: CollectionSettingsDialogProps) {
  const [activeTab, setActiveTab] = useState<SettingsTab>('auth');
  const [auth, setAuth] = useState<AuthState>(DEFAULT_AUTH);
  const [headers, setHeaders] = useState<KeyValueEntry[]>([]);

  async function handleSave() {
    try {
      const apiAuth = toApiAuth(auth);
      await saveCollectionSettings(collectionName, {
        auth: apiAuth.authType !== 'none' ? apiAuth : undefined,
        headers: headers.filter((h) => h.key).map((h) => ({
          key: h.key,
          value: h.value,
          enabled: h.enabled,
        })),
        variables: [],
      });
      onClose();
    } catch (err) {
      console.error('[CollectionSettings] save failed', err);
    }
  }

  return (
    <div className="flex flex-col gap-4 p-4">
      {/* Dialog header. */}
      <div>
        <h2 className="text-sm font-semibold">{collectionName}</h2>
        <p className="text-xs text-muted-foreground">Collection settings</p>
      </div>

      {/* Tab bar. */}
      <div className="flex gap-1 border-b border-border pb-0">
        {TABS.map((tab) => (
          <button
            key={tab.value}
            type="button"
            onClick={() => setActiveTab(tab.value)}
            className={cn(
              'px-3 pb-2 text-xs font-medium transition-colors',
              activeTab === tab.value
                ? 'border-b-2 border-primary text-foreground'
                : 'text-muted-foreground hover:text-foreground',
            )}
          >
            {tab.label}
          </button>
        ))}
      </div>

      {/* Tab content. */}
      <div className="min-h-[12rem]">
        {activeTab === 'auth' && (
          <AuthEditor auth={auth} onChange={setAuth} />
        )}
        {activeTab === 'headers' && (
          <HeadersEditor headers={headers} onChange={setHeaders} />
        )}
      </div>

      {/* Footer actions. */}
      <div className="flex justify-end gap-2">
        <Button variant="ghost" size="sm" onClick={onClose}>
          Cancel
        </Button>
        <Button size="sm" onClick={handleSave}>
          Save
        </Button>
      </div>
    </div>
  );
}
