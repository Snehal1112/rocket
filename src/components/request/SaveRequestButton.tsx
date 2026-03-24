import { useState, useEffect, useCallback } from 'react';
import { Save, ChevronDown } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { saveRequest } from '@/lib/tauri-api';
import { toApiAuth } from '@/lib/execute-request';
import { usePaneStore } from '@/stores/pane-store';
import { SaveToCollectionDialog } from '@/components/collections/SaveToCollectionDialog';
import type { Tab } from '@/types/pane-types';

interface SaveRequestButtonProps {
  tab: Tab;
  groupId: string;
}

// Builds the API-compatible request payload from the current tab state.
function buildRequestPayload(tab: Tab) {
  return {
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
    auth: toApiAuth(tab.request.auth),
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
      window.dispatchEvent(new CustomEvent('rocket:collections-changed'));
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
