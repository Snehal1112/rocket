import { useState, useEffect, useCallback } from 'react';
import { Save, ChevronDown } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { saveRequest, type Auth } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { SaveToCollectionDialog } from '@/components/collections/SaveToCollectionDialog';
import type { RequestTab } from '@/types/pane-types';

interface SaveRequestButtonProps {
  tab: RequestTab;
  groupId: string;
}

// Maps AuthState to a persistence-safe format that preserves the full config.
// Unlike toApiAuth (which converts oauth2 to bearer for execution), this
// keeps the original auth type and all fields so they survive save/reload.
function authForSave(auth: RequestTab['request']['auth']): Auth {
  switch (auth.authType) {
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

// Builds the request payload for saving to disk.
function buildRequestPayload(tab: RequestTab) {
  return {
    uid: '',
    name: tab.title,
    method: tab.request.method,
    url: tab.request.url,
    headers: tab.request.headers
      .filter((h) => h.key)
      .map((h) => ({ key: h.key, value: h.value, enabled: h.enabled })),
    body:
      tab.request.body.mode !== 'none'
        ? { mode: tab.request.body.mode, content: tab.request.body.content }
        : undefined,
    auth: authForSave(tab.request.auth),
  };
}

export function SaveRequestButton({ tab }: SaveRequestButtonProps) {
  const [saveAsOpen, setSaveAsOpen] = useState(false);
  const markClean = usePaneStore((s) => s.markClean);

  // Saves directly to the linked collection without prompting.
  const handleDirectSave = useCallback(async () => {
    if (!tab.source) return;
    try {
      await saveRequest(tab.source.collection, tab.source.path, buildRequestPayload(tab));
      markClean(tab.id);
    } catch (err) {
      console.error('[SaveRequestButton] Direct save failed:', err);
    }
  }, [tab, markClean]);

  // Listen for Cmd+S events dispatched from the global keyboard handler.
  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent<{ tabId: string }>).detail;
      if (detail?.tabId !== tab.id) return;
      if (tab.source) {
        void handleDirectSave();
      } else {
        setSaveAsOpen(true);
      }
    };
    window.addEventListener('rocket:save-draft', handler);
    return () => window.removeEventListener('rocket:save-draft', handler);
  }, [tab.id, tab.source, handleDirectSave]);

  // Draft tab — single button that opens the save-as dialog.
  if (!tab.source) {
    return (
      <>
        <Button
          size="sm"
          variant="outline"
          className="h-8 px-3"
          onClick={() => setSaveAsOpen(true)}
        >
          <Save className="mr-1 h-3.5 w-3.5" />
          Save
        </Button>
        <SaveToCollectionDialog
          open={saveAsOpen}
          onOpenChange={setSaveAsOpen}
          tabId={tab.id}
          title={tab.title}
          request={tab.request}
        />
      </>
    );
  }

  // Collection-linked tab — split button with direct save and save-as options.
  return (
    <>
      <div className="flex">
        <Button
          size="sm"
          variant="outline"
          className="h-8 px-3 rounded-r-none border-r-0"
          onClick={() => void handleDirectSave()}
        >
          <Save className="mr-1 h-3.5 w-3.5" />
          Save
        </Button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              size="sm"
              variant="outline"
              className="h-8 px-1.5 rounded-l-none"
            >
              <ChevronDown className="h-3.5 w-3.5" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onClick={() => void handleDirectSave()}>
              Save
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => setSaveAsOpen(true)}>
              Save as...
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      <SaveToCollectionDialog
        open={saveAsOpen}
        onOpenChange={setSaveAsOpen}
        tabId={tab.id}
        title={tab.title}
        request={tab.request}
      />
    </>
  );
}
