import { defaultKeymap } from '@codemirror/commands';
import { EditorState } from '@codemirror/state';
import { placeholder as cmPlaceholder, EditorView, keymap } from '@codemirror/view';
import { type ReactPortal, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useVariableCommit } from '@/hooks/useVariableCommit';
import type { ParsedCurl } from '@/lib/curl-parser';
import type { VariableScopeEntry, VariableSource } from '@/lib/url-variables';
import { cn } from '@/lib/utils';
import {
  closePopover,
  getActivePopover,
  type PopoverState,
  rocketTheme,
  rocketThemeDark,
  secretMask,
  setVariableContextEffect,
  singleLineFilter,
  urlTokens,
  variableAutocomplete,
  variableContextFacet,
  variableHighlight,
  variablePopoverExtension,
} from './extensions';
import { VariablePopover } from './VariablePopover';

export interface SingleLineEditorProps {
  /** Current text content (controlled). */
  value: string;
  /** Called on every content change. */
  onChange: (value: string) => void;
  /** Placeholder shown when editor is empty. */
  placeholder?: string;
  /** Additional CSS class for the editor wrapper. */
  className?: string;
  /** Disables editing. */
  disabled?: boolean;

  // ── Variable system ──────────────────────────────────────
  /** Scope-aware variable map. When undefined, no variable extensions load. */
  variableContext?: Map<string, VariableScopeEntry>;
  /** Called when user clicks "Navigate to source →" in popover. */
  onNavigateToSource?: (source: VariableSource | 'pathParam', key: string) => void;

  // ── Secret masking ───────────────────────────────────────
  /** When true, non-variable text is masked with ● characters. */
  isSecret?: boolean;

  // ── URL bar extras ───────────────────────────────────────
  /** Path parameter values for :param highlighting. */
  pathParams?: Record<string, string>;
  /** Query parameter values for ?key=value highlighting. */
  queryParams?: Record<string, string>;
  /** Called when a path param value is edited in the popover. */
  onPathParamChange?: (key: string, value: string) => void;
  /** Called when a curl command is pasted. */
  onCurlImport?: (parsed: ParsedCurl) => void;
  /** Called on Enter key (send request). */
  onSubmit?: () => void;
  /** Raw keydown handler for additional shortcuts. */
  onKeyDown?: (event: KeyboardEvent) => void;
}

/**
 * CodeMirror v6-based single-line editor with variable highlighting,
 * autocomplete, and inline popover editing.
 *
 * Drop-in replacement for VariableAwareInput and VariableAwareUrlInput.
 */
export function SingleLineEditor({
  value,
  onChange,
  placeholder,
  className,
  disabled,
  variableContext,
  onNavigateToSource,
  isSecret,
  pathParams,
  queryParams,
  onPathParamChange,
  onCurlImport,
  onSubmit,
  onKeyDown: _onKeyDown,
}: SingleLineEditorProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  // Track whether we're currently syncing from props to editor to avoid loops.
  const isSyncingRef = useRef(false);

  // Popover portal state.
  const [popoverState, setPopoverState] = useState<PopoverState | null>(null);
  const [popoverContainer, setPopoverContainer] = useState<HTMLElement | null>(null);

  const commitVariable = useVariableCommit();

  // Stable ref for callbacks that extensions need.
  const onSubmitRef = useRef(onSubmit);
  onSubmitRef.current = onSubmit;

  // Build extensions list. Memoized to avoid recreating on every render.
  // biome-ignore lint/correctness/useExhaustiveDependencies: extensions rebuild only when presence toggles, not on identity change.
  const extensions = useMemo(() => {
    const exts = [
      singleLineFilter,
      rocketTheme,
      rocketThemeDark,
      keymap.of(defaultKeymap),
      EditorView.updateListener.of((update) => {
        if (update.docChanged && !isSyncingRef.current) {
          onChangeRef.current(update.state.doc.toString());
        }
      }),
    ];

    if (placeholder) {
      exts.push(cmPlaceholder(placeholder));
    }

    if (onSubmit) {
      exts.push(
        keymap.of([
          {
            key: 'Enter',
            run: () => {
              onSubmitRef.current?.();
              return true;
            },
          },
        ]),
      );
    }

    if (variableContext) {
      exts.push(
        variableContextFacet.of(variableContext),
        variableHighlight(),
        variableAutocomplete(),
        variablePopoverExtension(),
      );
    }

    if (disabled) {
      exts.push(EditorState.readOnly.of(true));
    }

    // URL-specific extensions (only for URL bar).
    if (pathParams || queryParams || onCurlImport) {
      exts.push(
        urlTokens({
          pathParams,
          queryParams,
          onCurlImport,
        }),
      );
    }

    // Secret masking (bearer tokens, passwords).
    if (isSecret) {
      exts.push(secretMask());
    }

    return exts;
  }, [
    !!variableContext,
    !!onSubmit,
    !!disabled,
    placeholder,
    !!pathParams,
    !!queryParams,
    !!onCurlImport,
    !!isSecret,
  ]);

  // Create the EditorView on mount.
  // biome-ignore lint/correctness/useExhaustiveDependencies: initial doc only — live sync is in the value-sync effect below.
  useEffect(() => {
    if (!containerRef.current) return;

    const state = EditorState.create({
      doc: value,
      extensions,
    });

    const view = new EditorView({
      state,
      parent: containerRef.current,
    });

    viewRef.current = view;

    return () => {
      view.destroy();
      viewRef.current = null;
    };
  }, [extensions]);

  // Sync props.value → editor when it changes externally.
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    const currentDoc = view.state.doc.toString();
    if (currentDoc !== value) {
      isSyncingRef.current = true;
      view.dispatch({
        changes: { from: 0, to: currentDoc.length, insert: value },
      });
      isSyncingRef.current = false;
    }
  }, [value]);

  // Update the variable context facet when it changes.
  useEffect(() => {
    const view = viewRef.current;
    if (!view || !variableContext) return;
    view.dispatch({
      effects: setVariableContextEffect.of(variableContext),
    });
  }, [variableContext]);

  // Observe tooltip DOM for popover portal.
  // biome-ignore lint/correctness/useExhaustiveDependencies: re-setup when view is recreated via extensions dep.
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;

    const observer = new MutationObserver(() => {
      const container = view.dom.querySelector('.cm-variable-popover-container');
      const popover = getActivePopover(view);
      if (container && popover) {
        setPopoverContainer(container as HTMLElement);
        setPopoverState(popover);
      } else {
        setPopoverContainer(null);
        setPopoverState(null);
      }
    });

    // Observe the tooltip container area.
    const tooltipParent = view.dom.querySelector('.cm-tooltip-section')?.parentElement ?? view.dom;
    observer.observe(tooltipParent, { childList: true, subtree: true });

    // Also observe the root in case tooltips are added at a higher level.
    observer.observe(view.dom, { childList: true, subtree: true });

    return () => observer.disconnect();
  }, [extensions]);

  // Popover commit handler.
  const handlePopoverCommit = useCallback(
    async (newValue: string) => {
      if (!popoverState) return;
      if (popoverState.tokenType === 'pathParam' && onPathParamChange) {
        onPathParamChange(popoverState.varName, newValue);
      } else {
        await commitVariable(popoverState.varName, newValue, popoverState.entry?.source ?? null);
      }
    },
    [popoverState, commitVariable, onPathParamChange],
  );

  // Close popover handler.
  const handlePopoverClose = useCallback(() => {
    const view = viewRef.current;
    if (view) closePopover(view);
    setPopoverContainer(null);
    setPopoverState(null);
  }, []);

  // Render the popover portal if active.
  const popoverPortal: ReactPortal | null =
    popoverContainer && popoverState
      ? createPortal(
          <VariablePopover
            varName={popoverState.varName}
            entry={popoverState.entry}
            tokenType={popoverState.tokenType}
            onCommit={handlePopoverCommit}
            onClose={handlePopoverClose}
            onNavigateToSource={onNavigateToSource}
          />,
          popoverContainer,
        )
      : null;

  return (
    <div
      className={cn(
        'h-8 rounded-md border border-input bg-background overflow-hidden',
        'focus-within:ring-[3px] focus-within:ring-ring/50 focus-within:border-ring',
        disabled && 'cursor-not-allowed opacity-50',
        className,
      )}
    >
      <div ref={containerRef} className='h-full' />
      {popoverPortal}
    </div>
  );
}
