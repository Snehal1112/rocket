import { toast } from 'sonner';
import { snapOf } from '@/lib/flow-history';
import { cleanSaveMessage } from '@/lib/flow-issues';
import { flowPayloadFromTab } from '@/lib/flow-save';
import { saveFlow, saveScriptFile } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { FlowTab, ScriptTab } from '@/types/pane-types';

const messageOf = (err: unknown): string => (err instanceof Error ? err.message : String(err));

// Saves a script tab's file. Resolves true when it was written.
export async function saveScriptTab(tab: ScriptTab): Promise<boolean> {
  try {
    await saveScriptFile(tab.collectionName, tab.scriptPath, tab.content);
    usePaneStore.getState().markScriptSaved(tab.id, tab.content);
    return true;
  } catch (err) {
    toast.error(`Could not save "${tab.title}": ${messageOf(err)}`);
    return false;
  }
}

// Saves a flow tab. Resolves true when it was written. The graph being written
// is remembered, so an edit made during the save stays unsaved.
export async function saveFlowTab(tab: FlowTab): Promise<boolean> {
  const payload = flowPayloadFromTab(tab);
  if (!payload) {
    toast.error(`Could not save "${tab.title}": no flow is open in this tab.`);
    return false;
  }
  const written = snapOf(tab);
  try {
    await saveFlow(payload.collection, payload.flow);
    usePaneStore.getState().markClean(tab.id, written);
    return true;
  } catch (err) {
    toast.error(`Could not save "${tab.title}": ${cleanSaveMessage(messageOf(err))}`);
    return false;
  }
}
