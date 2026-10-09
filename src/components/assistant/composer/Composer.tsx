import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { toast } from 'sonner';
import { sendAssistantMessage, stopAssistantTurn } from '@/lib/assistant/assistant-session';
import { chipToResource, isChipLoadFailure } from '@/lib/assistant/chip-resources';
import { rememberModel } from '@/lib/assistant/model-memory';
import {
  appendPromptHistory,
  loadPromptHistory,
  savePromptHistory,
} from '@/lib/assistant/prompt-history';
import { filterSlashCommands } from '@/lib/assistant/slash-commands';
import type { ReferenceItem } from '@/lib/assistant/types';
import {
  type ConfigOption,
  type PromptResourceDto,
  setAgentConfigOption,
  setAssistantMode,
} from '@/lib/tauri-api';
import { selectTurnRunning, useAssistantStore } from '@/stores/assistant-store';
import { usePaneStore } from '@/stores/pane-store';
import { useWorkspaceStore } from '@/stores/workspace-store';
import { ComposerChips } from './ComposerChips';
import { type AssistantModeValue, ComposerToolbar, MODEL_OPTION_ID } from './ComposerToolbar';
import { addChip, type ComposerChip, chipKey, MAX_CHIPS, removeChip } from './chips';
import { PromptEditor } from './PromptEditor';
import { matchTrigger } from './prompt-triggers';
import { filterReferences, focusReference, lastResponseReference } from './reference-source';
import { useReferenceItems } from './useReferenceItems';

const NO_OPTIONS: ConfigOption[] = [];

/** True while the last line of the prompt is a `#` reference being typed. */
function isReferencePickerOpen(text: string): boolean {
  const lineStart = text.lastIndexOf('\n') + 1;
  return matchTrigger(text.slice(lineStart), lineStart)?.kind === 'reference';
}

/**
 * The AI Assistant prompt area: chips, the prompt editor and the toolbar. Chips become
 * masked text resources (built by the backend), then `sendAssistantMessage` runs the turn.
 */
export function Composer() {
  const sessionId = useAssistantStore((s) => s.session?.sessionId);
  const sessionActive = useAssistantStore((s) => s.session?.status === 'active');
  const agentConfigId = useAssistantStore((s) => s.session?.agentConfigId);
  const mode = useAssistantStore((s) => s.session?.mode ?? 'ask');
  const configOptions = useAssistantStore((s) => s.session?.configOptions) ?? NO_OPTIONS;
  const usage = useAssistantStore((s) => s.usage);
  const focus = useAssistantStore((s) => s.focus);
  const turnRunning = useAssistantStore(selectTurnRunning);
  const setConfigOptions = useAssistantStore((s) => s.setConfigOptions);
  const setMode = useAssistantStore((s) => s.setMode);
  const workspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);

  const [text, setText] = useState('');
  const [sending, setSending] = useState(false);
  // A turn runs while this composer's send is in flight or the store's reply still streams.
  const running = sending || turnRunning;
  const [extraChips, setExtraChips] = useState<ComposerChip[]>([]);
  const [dismissedFocusKey, setDismissedFocusKey] = useState<string | null>(null);
  const [history, setHistory] = useState<string[]>(() => loadPromptHistory(workspaceId));
  // Refs answer "what is typed" and "is a turn running" at once, before React re-renders,
  // so two quick Enters cannot start two turns.
  const textRef = useRef('');
  const runningRef = useRef(false);

  // Each workspace keeps its own prompt history. A switch also drops the draft and chips.
  const shownWorkspaceRef = useRef(workspaceId);
  useEffect(() => {
    setHistory(loadPromptHistory(workspaceId));
    if (shownWorkspaceRef.current !== workspaceId) {
      shownWorkspaceRef.current = workspaceId;
      textRef.current = '';
      setText('');
      setExtraChips([]);
      setDismissedFocusKey(null);
    }
  }, [workspaceId]);

  // Trees reload each time the `#` picker opens.
  const referenceItems = useReferenceItems(isReferencePickerOpen(text));

  // The pane store is read only to label the focus chip with the tab title.
  const focusItem = useMemo(() => focusReference(focus, usePaneStore.getState().root), [focus]);

  const chips = useMemo<ComposerChip[]>(() => {
    if (!focusItem || chipKey(focusItem) === dismissedFocusKey) return extraChips;
    return [{ key: chipKey(focusItem), item: focusItem, focus: true }, ...extraChips];
  }, [focusItem, dismissedFocusKey, extraChips]);

  const referenceSource = useCallback(
    (query: string) => {
      const lastResponse = lastResponseReference(focus, usePaneStore.getState().root);
      return filterReferences(
        lastResponse ? [lastResponse, ...referenceItems] : referenceItems,
        query,
      );
    },
    [focus, referenceItems],
  );

  const handleChange = (next: string) => {
    textRef.current = next;
    setText(next);
  };

  const handleReferencePicked = (item: ReferenceItem) => {
    const result = addChip(chips, item);
    if (result.outcome === 'limit') {
      toast.warning(`A message can carry at most ${MAX_CHIPS} references.`);
      return;
    }
    if (result.outcome === 'added') setExtraChips(result.chips.filter((chip) => !chip.focus));
  };

  const handleRemoveChip = (key: string) => {
    if (chips.some((chip) => chip.key === key && chip.focus)) {
      setDismissedFocusKey(key);
      return;
    }
    setExtraChips((current) => removeChip(current, key));
  };

  const handleHistoryCommit = (prompt: string) => {
    const next = appendPromptHistory(history, prompt);
    setHistory(next);
    savePromptHistory(workspaceId, next);
  };

  const handleSend = async () => {
    const prompt = textRef.current.trim();
    if (!sessionId || !sessionActive || runningRef.current || running || prompt === '') return;
    runningRef.current = true;
    setSending(true);
    try {
      // chipToResource never rejects and caps each text at 8 KB.
      const resources: PromptResourceDto[] = await Promise.all(
        chips.slice(0, MAX_CHIPS).map((chip) => chipToResource(chip.item)),
      );
      if (resources.some(isChipLoadFailure)) {
        // The prompt and chips stay, so nothing is sent without the context asked for.
        toast.error('A reference could not be loaded, so the message was not sent. Try again.');
        return;
      }
      textRef.current = '';
      setText('');
      setExtraChips([]);
      // The focus chip comes back for the next message.
      setDismissedFocusKey(null);
      // The send flow adds the user message, refuses a second turn and fails the turn
      // (not the session) when the send throws.
      await sendAssistantMessage(prompt, resources.length > 0 ? resources : undefined);
    } catch (err) {
      toast.error(`The message could not be sent: ${String(err)}`);
    } finally {
      runningRef.current = false;
      setSending(false);
    }
  };

  const handleStop = () => {
    void stopAssistantTurn();
  };

  const handleModeChange = async (next: AssistantModeValue) => {
    if (!sessionId || next === mode) return;
    try {
      await setAssistantMode(sessionId, next);
      setMode(next);
    } catch (err) {
      toast.error(`Could not switch to ${next} mode: ${String(err)}`);
    }
  };

  const handleConfigChange = async (configId: string, value: string) => {
    if (!sessionId) return;
    try {
      // The reply is the full option list, so Effort appears or disappears with the model.
      const options = await setAgentConfigOption(sessionId, configId, value);
      setConfigOptions(sessionId, options);
      if (configId === MODEL_OPTION_ID && agentConfigId) rememberModel(agentConfigId, value);
    } catch (err) {
      toast.error(`Could not change the ${configId}: ${String(err)}`);
    }
  };

  return (
    <div className='m-2 shrink-0 rounded-md border bg-dropdown-bg shadow-xs focus-within:border-ring focus-within:ring-[3px] focus-within:ring-ring/50 dark:bg-input/30'>
      <ComposerChips chips={chips} onRemove={handleRemoveChip} />
      <PromptEditor
        value={text}
        onChange={handleChange}
        onSubmit={() => void handleSend()}
        onStop={handleStop}
        running={running}
        placeholder='Ask about this workspace. Type # to add context or / for a template.'
        disabled={!sessionActive}
        history={history}
        onHistoryCommit={handleHistoryCommit}
        referenceSource={referenceSource}
        commandSource={filterSlashCommands}
        onReferencePicked={handleReferencePicked}
        aria-label='Message the AI assistant'
      />
      <ComposerToolbar
        mode={mode}
        onModeChange={(next) => void handleModeChange(next)}
        configOptions={configOptions}
        onConfigChange={(configId, value) => void handleConfigChange(configId, value)}
        usage={usage}
        running={running}
        canSend={sessionActive && !running && text.trim() !== ''}
        disabled={!sessionActive}
        onSend={() => void handleSend()}
        onStop={handleStop}
      />
    </div>
  );
}
