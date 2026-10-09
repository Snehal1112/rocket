import { autocompletion, completionStatus } from '@codemirror/autocomplete';
import { defaultKeymap, history, historyKeymap, insertNewline } from '@codemirror/commands';
import { Annotation, Compartment, EditorState, type Extension, Prec } from '@codemirror/state';
import { placeholder as cmPlaceholder, EditorView, keymap, tooltips } from '@codemirror/view';
import { useEffect, useLayoutEffect, useMemo, useRef } from 'react';
import {
  rocketTheme,
  rocketThemeDark,
  rocketTooltipBase,
  setVariableContextEffect,
  variableCompletionSource,
  variableContextField,
  variableHighlight,
} from '@/components/editor/extensions';
import type { ReferenceItem, SlashCommandItem } from '@/lib/assistant/types';
import type { VariableScopeEntry } from '@/lib/url-variables';
import { cn } from '@/lib/utils';
import { commandCompletions, referenceCompletions } from './prompt-completions';
import { promptEditorTheme } from './prompt-editor-theme';
import {
  atRecallEdge,
  type HistoryCursor,
  type HistoryDirection,
  IDLE_HISTORY_CURSOR,
  stepHistory,
} from './prompt-history-nav';

export type { ReferenceItem, SlashCommandItem } from '@/lib/assistant/types';

export interface PromptEditorProps {
  value: string;
  onChange(v: string): void;
  onSubmit(): void;
  onStop(): void;
  running: boolean;
  placeholder?: string;
  disabled?: boolean;
  history: string[];
  onHistoryCommit(v: string): void;
  referenceSource: (query: string) => ReferenceItem[];
  commandSource: (query: string) => SlashCommandItem[];
  onReferencePicked(item: ReferenceItem): void;
  variableContext?: Map<string, VariableScopeEntry>;
  'aria-label': string;
}

function ariaExtension(label: string): Extension {
  return EditorView.contentAttributes.of({ 'aria-label': label, 'aria-multiline': 'true' });
}

function editableExtension(disabled: boolean): Extension {
  return disabled ? [EditorState.readOnly.of(true), EditorView.editable.of(false)] : [];
}

// Marks the change a history recall makes, so it does not reset the recall cursor.
const historyRecall = Annotation.define<boolean>();

/**
 * The AI Assistant prompt box: CodeMirror 6, multi-line, growing to a maximum height.
 * This is the one approved CodeMirror exception for multi-line text (see
 * `.claude/rules/frontend-component-guardrails.md`). Do not reuse it elsewhere.
 */
export function PromptEditor({
  value,
  onChange,
  onSubmit,
  onStop,
  running,
  placeholder,
  disabled,
  history: promptHistory,
  onHistoryCommit,
  referenceSource,
  commandSource,
  onReferencePicked,
  variableContext,
  'aria-label': ariaLabel,
}: PromptEditorProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  // True while props.value is pushed into the editor, so onChange does not echo it back.
  const isSyncingRef = useRef(false);
  const historyCursorRef = useRef<HistoryCursor>(IDLE_HISTORY_CURSOR);

  // Cosmetic and state props live in compartments, so changing them keeps the view.
  const ariaCompartment = useRef(new Compartment());
  const placeholderCompartment = useRef(new Compartment());
  const editableCompartment = useRef(new Compartment());

  // The extensions are built once, so they read the latest props through refs.
  const propsRef = useRef({ onChange, onSubmit, onStop, running, promptHistory, onHistoryCommit });
  const referenceSourceRef = useRef(referenceSource);
  const commandSourceRef = useRef(commandSource);
  const onReferencePickedRef = useRef(onReferencePicked);
  const variableContextRef = useRef(variableContext);
  // Refs update after render commits, so a discarded concurrent render never leaks in.
  useLayoutEffect(() => {
    propsRef.current = { onChange, onSubmit, onStop, running, promptHistory, onHistoryCommit };
    referenceSourceRef.current = referenceSource;
    commandSourceRef.current = commandSource;
    onReferencePickedRef.current = onReferencePicked;
    variableContextRef.current = variableContext;
  });

  // biome-ignore lint/correctness/useExhaustiveDependencies: extensions rebuild only when presence toggles, not on identity change.
  const extensions = useMemo(() => {
    const submit = (view: EditorView): boolean => {
      // Enter during IME composition confirms the text and must not send.
      if (view.composing || view.compositionStarted) return false;
      const props = propsRef.current;
      // A running turn ignores Enter, so one turn never queues a second prompt.
      if (props.running) return true;
      const text = view.state.doc.toString();
      if (text.trim() === '') return true;
      historyCursorRef.current = IDLE_HISTORY_CURSOR;
      props.onHistoryCommit(text);
      props.onSubmit();
      return true;
    };

    const stop = (): boolean => {
      if (!propsRef.current.running) return false;
      propsRef.current.onStop();
      return true;
    };

    const recall =
      (direction: HistoryDirection) =>
      (view: EditorView): boolean => {
        if (completionStatus(view.state) !== null) return false;
        if (!atRecallEdge(view.state, direction)) return false;
        const step = stepHistory(
          propsRef.current.promptHistory,
          historyCursorRef.current,
          view.state.doc.toString(),
          direction,
        );
        if (!step) return false;
        historyCursorRef.current = step.cursor;
        view.dispatch({
          changes: { from: 0, to: view.state.doc.length, insert: step.text },
          selection: { anchor: step.text.length },
          annotations: historyRecall.of(true),
        });
        return true;
      };

    const exts: Extension[] = [
      rocketTheme,
      rocketThemeDark,
      rocketTooltipBase,
      Prec.high(promptEditorTheme),
      EditorView.lineWrapping,
      history(),
      // Keep Enter inside the editor, so the window's Cmd/Ctrl+Enter shortcut does not
      // also send the active HTTP request.
      Prec.highest(
        EditorView.domEventHandlers({
          keydown(event) {
            if (event.key === 'Enter') event.stopPropagation();
            return false;
          },
        }),
      ),
      Prec.high(
        keymap.of([
          { key: 'Enter', run: submit },
          { key: 'Mod-Enter', run: submit },
          { key: 'Shift-Enter', run: insertNewline },
          { key: 'Escape', run: stop },
          { key: 'ArrowUp', run: recall('up') },
          { key: 'ArrowDown', run: recall('down') },
        ]),
      ),
      keymap.of([...defaultKeymap, ...historyKeymap]),
      // Render the completion list at document root so it escapes the panel's overflow.
      tooltips({ parent: document.body }),
      // One autocompletion() only: CodeMirror cannot merge two different `override` lists.
      autocompletion({
        override: [
          referenceCompletions(referenceSourceRef, onReferencePickedRef),
          commandCompletions(commandSourceRef),
          ...(variableContext ? [variableCompletionSource] : []),
        ],
        activateOnTyping: true,
        icons: false,
      }),
      EditorView.updateListener.of((update) => {
        if (!update.docChanged || isSyncingRef.current) return;
        const recalled = update.transactions.some((tr) => tr.annotation(historyRecall));
        if (!recalled) historyCursorRef.current = IDLE_HISTORY_CURSOR;
        propsRef.current.onChange(update.state.doc.toString());
      }),
      ariaCompartment.current.of(ariaExtension(ariaLabel)),
      placeholderCompartment.current.of(placeholder ? cmPlaceholder(placeholder) : []),
      editableCompartment.current.of(editableExtension(!!disabled)),
    ];

    if (variableContext) exts.push(variableContextField, variableHighlight());
    return exts;
  }, [!!variableContext]);

  // Create the EditorView. It is rebuilt only when the extension set changes.
  // biome-ignore lint/correctness/useExhaustiveDependencies: initial doc only, live sync is in the value effect below.
  useEffect(() => {
    if (!containerRef.current) return;
    let state = EditorState.create({ doc: value, extensions });
    // Seed the variable context before the view exists, so highlighting starts correct.
    if (variableContextRef.current) {
      state = state.update({
        effects: setVariableContextEffect.of(variableContextRef.current),
      }).state;
    }
    const view = new EditorView({ state, parent: containerRef.current });
    viewRef.current = view;
    return () => {
      view.destroy();
      viewRef.current = null;
    };
  }, [extensions]);

  // Push an outside value change (for example clearing after send) into the editor.
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    const currentDoc = view.state.doc.toString();
    if (currentDoc !== value) {
      isSyncingRef.current = true;
      view.dispatch({ changes: { from: 0, to: currentDoc.length, insert: value } });
      isSyncingRef.current = false;
    }
  }, [value]);

  // Reconfigure the compartments in place, so undo history and the cursor survive.
  useEffect(() => {
    viewRef.current?.dispatch({
      effects: [
        ariaCompartment.current.reconfigure(ariaExtension(ariaLabel)),
        placeholderCompartment.current.reconfigure(placeholder ? cmPlaceholder(placeholder) : []),
        editableCompartment.current.reconfigure(editableExtension(!!disabled)),
      ],
    });
  }, [ariaLabel, placeholder, disabled]);

  // Keep the highlight context current.
  useEffect(() => {
    const view = viewRef.current;
    if (!view || !variableContext) return;
    view.dispatch({ effects: setVariableContextEffect.of(variableContext) });
  }, [variableContext]);

  return (
    <div ref={containerRef} className={cn('w-full', disabled && 'cursor-not-allowed opacity-50')} />
  );
}
