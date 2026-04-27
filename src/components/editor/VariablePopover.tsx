import { useCallback, useEffect, useRef, useState } from 'react';
import { Input } from '@/components/ui/input';
import { generateDynamicVar, isDynamicVar } from '@/lib/dynamic-vars';
import {
  sourceBadgeClass,
  type VariableScopeEntry,
  type VariableSource,
} from '@/lib/url-variables';
import { cn } from '@/lib/utils';

export interface VariablePopoverProps {
  /** The variable name (without {{ }}). */
  varName: string;
  /** Resolved scope entry, or undefined if unresolved. */
  entry: VariableScopeEntry | undefined;
  /** Token type — 'variable' or 'pathParam'. */
  tokenType: 'variable' | 'pathParam';
  /** Called to save the edited value. */
  onCommit: (newValue: string) => Promise<void>;
  /** Called to close the popover without saving. */
  onClose: () => void;
  /** Called when user clicks "Navigate to source →". */
  onNavigateToSource?: (source: VariableSource | 'pathParam', key: string) => void;
}

/** Navigation link label for a given scope. */
function navLinkLabel(source: VariableSource | 'pathParam'): string | null {
  switch (source) {
    case 'pathParam':
      return 'Params →';
    case 'request':
    case 'runtime':
      return 'Request Variables →';
    case 'environment':
      return 'Collection Environments →';
    case 'global':
      return 'Global Environments →';
    case 'collection':
      return 'Collection Variables →';
    default:
      return null; // folder, process — no navigation available
  }
}

/** Whether a scope is editable in the popover. */
function isEditable(entry: VariableScopeEntry | undefined): boolean {
  if (!entry) return true; // Unresolved — editable (creates in active env)
  return entry.source === 'environment' || entry.source === 'global';
}

/**
 * Popover content for a variable or path param token.
 * Rendered via React portal into a CM6 tooltip DOM container.
 *
 * Layout matches the existing VariableAwareUrlInput popover:
 * - Value input (editable or read-only based on scope)
 * - Footer: scope badge + label (left), "Navigate to source →" link (right)
 */
export function VariablePopover({
  varName,
  entry,
  tokenType,
  onCommit,
  onClose,
  onNavigateToSource,
}: VariablePopoverProps) {
  // Synthesize an entry for $-prefixed dynamic variables. The scope map
  // does not contain dynamic vars — they are generated fresh per-render.
  let resolvedEntry = entry;
  if (tokenType === 'variable' && varName.startsWith('$')) {
    const stripped = varName.slice(1);
    if (isDynamicVar(stripped)) {
      resolvedEntry = {
        value: generateDynamicVar(stripped) ?? '',
        source: 'dynamic',
        label: 'Dynamic',
        secret: false,
      };
    }
  }

  const [editValue, setEditValue] = useState(
    resolvedEntry?.secret ? '' : (resolvedEntry?.value ?? ''),
  );
  const inputRef = useRef<HTMLInputElement>(null);
  const committedRef = useRef(false);

  // Auto-focus the input on mount.
  useEffect(() => {
    // Small delay to let the tooltip DOM settle before focusing.
    const id = requestAnimationFrame(() => inputRef.current?.focus());
    return () => cancelAnimationFrame(id);
  }, []);

  const readOnly = resolvedEntry?.secret || !isEditable(resolvedEntry);

  const handleCommit = useCallback(async () => {
    if (committedRef.current || readOnly) return;
    committedRef.current = true;
    await onCommit(editValue);
    onClose();
  }, [editValue, onCommit, onClose, readOnly]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        void handleCommit();
      }
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      }
    },
    [handleCommit, onClose],
  );

  // Scope badge + label
  const scopeSource: VariableSource | 'pathParam' | null =
    tokenType === 'pathParam' ? 'pathParam' : (resolvedEntry?.source ?? null);
  const linkLabel = scopeSource !== null ? navLinkLabel(scopeSource) : null;

  const badgeIcon =
    tokenType === 'pathParam'
      ? ':'
      : resolvedEntry
        ? resolvedEntry.source.charAt(0).toUpperCase()
        : '?';

  const badgeClass =
    tokenType === 'pathParam'
      ? 'text-violet-500 font-bold text-xs'
      : resolvedEntry
        ? cn(
            'rounded-full w-4 h-4 inline-flex items-center justify-center text-2xs font-bold',
            sourceBadgeClass(resolvedEntry.source),
          )
        : 'text-muted-foreground';

  const scopeLabel =
    tokenType === 'pathParam'
      ? 'Path Variable'
      : resolvedEntry
        ? resolvedEntry.label
        : 'Unresolved';

  return (
    <div
      role='dialog'
      aria-label={`Edit variable ${varName}`}
      className='min-w-80 w-max max-w-lg bg-card/50 backdrop-blur-sm text-popover-foreground rounded-sm border border-border shadow-[0_2px_8px_rgba(0,0,0,0.16)] dark:shadow-[0_2px_8px_rgba(0,0,0,0.4)] overflow-hidden'
      // Prevent click inside popover from bubbling to EditorView and closing it.
      onMouseDown={(e) => e.stopPropagation()}
    >
      {/* Value input — borderless: the popover card is the chrome. */}
      <div className='px-2 py-1.5'>
        <Input
          ref={inputRef}
          className='h-7 border-0 bg-transparent px-2 text-xs font-mono shadow-none focus-visible:border-0 focus-visible:ring-0 dark:bg-transparent'
          value={resolvedEntry?.secret ? '●●●●' : editValue}
          placeholder={resolvedEntry ? 'Value' : 'Not set'}
          readOnly={readOnly}
          onChange={(e) => {
            if (readOnly) return;
            setEditValue(e.target.value);
          }}
          onKeyDown={handleKeyDown}
          onBlur={() => void handleCommit()}
        />
      </div>

      {/* Footer: scope badge + nav link */}
      <div className='flex items-center justify-between gap-4 px-2 py-1.5 border-t border-border/50 bg-muted/30'>
        <div className='flex items-center gap-1.5 text-2xs text-muted-foreground whitespace-nowrap'>
          <span className={badgeClass}>{badgeIcon}</span>
          <span>{scopeLabel}</span>
        </div>
        {onNavigateToSource && scopeSource !== null && linkLabel !== null && (
          <button
            type='button'
            className='text-2xs text-primary hover:underline cursor-pointer whitespace-nowrap'
            onMouseDown={(e) => e.preventDefault()}
            onClick={async () => {
              await handleCommit();
              onNavigateToSource(scopeSource, varName);
            }}
          >
            {linkLabel}
          </button>
        )}
      </div>
    </div>
  );
}
