import { useState, useEffect, useCallback } from 'react';
import { Save, Check } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { saveRequest, type Auth, type Request as ApiRequest } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { RequestTab } from '@/types/pane-types';

interface SaveRequestButtonProps {
  tab: RequestTab;
  groupId: string;
}

// Maps AuthState to the flat Rust Auth shape for disk persistence.
function authForSave(auth: RequestTab['request']['auth']): Auth {
  switch (auth.authType) {
    case 'inherit':
    case 'none':
      return { authType: 'none' };
    case 'basic':
      return { authType: 'basic', username: auth.basic?.username ?? '', password: auth.basic?.password ?? '' };
    case 'bearer':
      return { authType: 'bearer', token: auth.bearer?.token ?? '' };
    case 'api-key':
      return { authType: 'api-key', key: auth.apiKey?.key ?? '', value: auth.apiKey?.value ?? '', addTo: auth.apiKey?.addTo ?? 'header' };
    case 'oauth2':
      return {
        authType: 'oauth2',
        grantType: auth.oauth2?.grantType ?? 'client_credentials',
        authorizationUrl: auth.oauth2?.authorizationUrl ?? '',
        tokenUrl: auth.oauth2?.tokenUrl ?? '',
        callbackUrl: auth.oauth2?.callbackUrl ?? '',
        clientId: auth.oauth2?.clientId ?? '',
        clientSecret: auth.oauth2?.clientSecret ?? '',
        scope: auth.oauth2?.scope ?? '',
        state: auth.oauth2?.state ?? '',
        username: auth.oauth2?.username ?? '',
        password: auth.oauth2?.password ?? '',
        clientAuthentication: auth.oauth2?.clientAuthentication ?? 'body',
        headerPrefix: auth.oauth2?.headerPrefix ?? 'Bearer',
        addTokenTo: auth.oauth2?.addTokenTo ?? 'header',
        verifySsl: auth.oauth2?.verifySsl ?? true,
        accessToken: auth.oauth2?.accessToken ?? '',
        refreshToken: auth.oauth2?.refreshToken ?? '',
        expiresIn: auth.oauth2?.expiresIn ?? null,
        tokenAcquiredAt: auth.oauth2?.tokenAcquiredAt ?? null,
      };
    case 'aws-sig-v4':
      return {
        authType: 'aws-sig-v4',
        accessKey: auth.awsSigV4?.accessKey ?? '',
        secretKey: auth.awsSigV4?.secretKey ?? '',
        region: auth.awsSigV4?.region ?? '',
        service: auth.awsSigV4?.service ?? '',
        sessionToken: auth.awsSigV4?.sessionToken ?? '',
      };
    default:
      return { authType: 'none' };
  }
}

function buildPayloadFromTab(tab: RequestTab): ApiRequest {
  const body = tab.request.body;
  return {
    uid: tab.id,
    name: tab.title,
    method: tab.request.method,
    url: tab.request.url,
    headers: tab.request.headers
      .filter((h) => h.key)
      .map((h) => ({ key: h.key, value: h.value, enabled: h.enabled })),
    body: body.mode !== 'none' ? { mode: body.mode, content: body.content } : undefined,
    auth: authForSave(tab.request.auth),
  };
}

export function SaveRequestButton({ tab }: SaveRequestButtonProps) {
  const markClean = usePaneStore((s) => s.markClean);
  const [saveStatus, setSaveStatus] = useState<'idle' | 'success' | 'error'>('idle');

  const handleSave = useCallback(async () => {
    if (!tab.source) return;
    try {
      await saveRequest(tab.source.collection, tab.source.path, buildPayloadFromTab(tab));
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
    <div className="flex items-center gap-1.5">
      <Button
        size="sm"
        variant="outline"
        className="h-8 px-3"
        disabled={!tab.isDirty}
        onClick={() => void handleSave()}
      >
        {saveStatus === 'success' ? (
          <Check className="mr-1 h-3.5 w-3.5 text-green-500" />
        ) : (
          <Save className="mr-1 h-3.5 w-3.5" />
        )}
        Save
      </Button>
      {saveStatus === 'error' && (
        <span className="text-2xs text-destructive">Save failed</span>
      )}
    </div>
  );
}
