import { defaultKeymap } from '@codemirror/commands';
import { EditorState } from '@codemirror/state';
import { placeholder as cmPlaceholder, EditorView, keymap, tooltips } from '@codemirror/view';
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
  rocketTooltipBase,
  secretMask,
  setVariableContextEffect,
  singleLineFilter,
  urlTokens,
  variableAutocomplete,
  variableContextField,
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
  /** Accessible label forwarded to the CodeMirror contenteditable element. */
  'aria-label'?: string;
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
  'aria-label': ariaLabel,
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
  // Ref always holds the latest popover state so the commit callback doesn't capture
  // a stale null after React re-renders between mousedown and blur.
  const popoverStateRef = useRef<PopoverState | null>(null);

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
      rocketTooltipBase,
      keymap.of(defaultKeymap),
      // Render tooltips at document root so the popover escapes
      // our overflow-hidden editor wrapper and any transformed ancestors.
      tooltips({ parent: document.body }),
      EditorView.updateListener.of((update) => {
        if (update.docChanged && !isSyncingRef.current) {
          onChangeRef.current(update.state.doc.toString());
        }
      }),
      // Forward aria-label to the CM contenteditable element when provided.
      ...(ariaLabel ? [EditorView.contentAttributes.of({ 'aria-label': ariaLabel })] : []),
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
        variableContextField,
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
    ariaLabel,
  ]);

  // Stable ref so the creation effect can read the current context without
  // being in its dependency list (creation runs only when extensions rebuild).
  const variableContextRef = useRef(variableContext);
  variableContextRef.current = variableContext;

  // Create the EditorView on mount.
  // biome-ignore lint/correctness/useExhaustiveDependencies: initial doc only — live sync is in the value-sync effect below.
  useEffect(() => {
    if (!containerRef.current) return;

    let state = EditorState.create({
      doc: value,
      extensions,
    });

    // Seed the variable context field before view creation so the highlight
    // plugin's constructor already sees the correct context (not empty Map).
    if (variableContextRef.current) {
      state = state.update({
        effects: setVariableContextEffect.of(variableContextRef.current),
      }).state;
    }

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

  // Update the variable context field when it changes.
  useEffect(() => {
    const view = viewRef.current;
    if (!view || !variableContext) return;
    view.dispatch({
      effects: setVariableContextEffect.of(variableContext),
    });
  }, [variableContext]);

  // Observe tooltip DOM for popover portal. Tooltips are mounted into
  // document.body via the tooltips() extension, so we watch the body root.
  // biome-ignore lint/correctness/useExhaustiveDependencies: re-setup when view is recreated via extensions dep.
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;

    const observer = new MutationObserver(() => {
      const container = document.querySelector('.cm-variable-popover-container');
      const popover = getActivePopover(view);
      if (container && popover) {
        popoverStateRef.current = popover;
        setPopoverContainer(container as HTMLElement);
        setPopoverState(popover);
      } else {
        popoverStateRef.current = null;
        setPopoverContainer(null);
        setPopoverState(null);
      }
    });

    observer.observe(document.body, { childList: true, subtree: true });

    return () => observer.disconnect();
  }, [extensions]);

  // Popover commit handler — reads from ref to avoid stale closure when React
  // re-renders between mousedown (which clears popoverState) and the subsequent blur.
  const handlePopoverCommit = useCallback(
    async (newValue: string) => {
      const ps = popoverStateRef.current;
      if (!ps) return;
      if (ps.tokenType === 'pathParam' && onPathParamChange) {
        onPathParamChange(ps.varName, newValue);
      } else {
        await commitVariable(ps.varName, newValue, ps.entry?.source ?? null);
      }
    },
    [commitVariable, onPathParamChange],
  );

  // Close popover handler.
  const handlePopoverClose = useCallback(() => {
    const view = viewRef.current;
    if (view) closePopover(view);
    popoverStateRef.current = null;
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
        'h-9 rounded-md border border-input bg-card dark:bg-input/30 overflow-hidden',
        'shadow-xs transition-[color,box-shadow]',
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
