import { useCallback, useEffect, useRef, useState } from 'react';
import { Input } from '@/components/ui/input';
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
  const [editValue, setEditValue] = useState(entry?.secret ? '' : (entry?.value ?? ''));
  const inputRef = useRef<HTMLInputElement>(null);
  const committedRef = useRef(false);

  // Auto-focus the input on mount.
  useEffect(() => {
    // Small delay to let the tooltip DOM settle before focusing.
    const id = requestAnimationFrame(() => inputRef.current?.focus());
    return () => cancelAnimationFrame(id);
  }, []);

  const readOnly = entry?.secret || !isEditable(entry);

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
    tokenType === 'pathParam' ? 'pathParam' : (entry?.source ?? null);
  const linkLabel = scopeSource !== null ? navLinkLabel(scopeSource) : null;

  const badgeIcon =
    tokenType === 'pathParam' ? ':' : entry ? entry.source.charAt(0).toUpperCase() : '?';

  const badgeClass =
    tokenType === 'pathParam'
      ? 'text-violet-500 font-bold text-xs'
      : entry
        ? cn(
            'rounded-full w-4 h-4 inline-flex items-center justify-center text-2xs font-bold',
            sourceBadgeClass(entry.source),
          )
        : 'text-muted-foreground';

  const scopeLabel =
    tokenType === 'pathParam' ? 'Path Variable' : entry ? entry.label : 'Unresolved';

  return (
    <div
      role='dialog'
      aria-label={`Edit variable ${varName}`}
      className='w-80 bg-popover text-popover-foreground rounded-md border shadow-md overflow-hidden'
      // Prevent click inside popover from bubbling to EditorView and closing it.
      onMouseDown={(e) => e.stopPropagation()}
    >
      {/* Value input */}
      <div className='p-2'>
        <Input
          ref={inputRef}
          className='h-7 text-xs font-mono'
          value={entry?.secret ? '●●●●' : editValue}
          placeholder={entry ? 'Value' : 'Not set'}
          readOnly={readOnly}
          onChange={(e) => {
            if (readOnly) return;
            setEditValue(e.target.value);
          }}
          onKeyDown={handleKeyDown}
          onBlur={() => {
            handleCommit();
          }}
        />
      </div>

      {/* Footer: scope badge + nav link */}
      <div className='flex items-center justify-between px-2 py-1.5 border-t border-border/50 bg-muted/30'>
        <div className='flex items-center gap-1.5 text-2xs text-muted-foreground'>
          <span className={badgeClass}>{badgeIcon}</span>
          <span>{scopeLabel}</span>
        </div>
        {onNavigateToSource && scopeSource !== null && linkLabel !== null && (
          <button
            type='button'
            className='text-2xs text-primary hover:underline cursor-pointer'
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
